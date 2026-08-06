#!/usr/bin/env python3
"""Bridge ESP32 Pocket Pi model decisions to a logged-in Mac CLI."""

from __future__ import annotations

import argparse
import getpass
import json
import os
import select
import signal
import subprocess
import termios
import time
import tty

from uart_bridge import create_backend

CONFIG_REQUEST = "PPI-CONFIG-REQUEST"
CONFIG_RESPONSE = "PPI-CONFIG:"
READY = "PPI-RPC-READY"
WAITING = "PPI-RPC-WAITING"
REQUEST = "PPI-RPC-REQUEST:"
STREAM = "PPI-RPC-STREAM:"


def write_line(fd: int, line: str) -> None:
    payload = memoryview(f"{line}\r\n".encode())
    while payload:
        try:
            count = os.write(fd, payload)
        except BlockingIOError:
            select.select([], [fd], [], 0.5)
            continue
        payload = payload[count:]


def config(args: argparse.Namespace) -> dict[str, object]:
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    value: dict[str, object] = {
        "modelBackend": args.backend,
        "modelProvider": provider,
        "unixTimeSeconds": int(time.time()),
    }
    if args.model:
        value["model"] = args.model
    if args.provision_wifi:
        value["wifiSsid"] = input("Wi-Fi SSID: ").strip()
        value["wifiPassword"] = getpass.getpass("Wi-Fi password: ")
    if args.backend == "wireless":
        value["modelApiKey"] = getpass.getpass(f"{provider} API key: ")
    if args.prompt:
        value["initialPrompt"] = args.prompt
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument("--backend", choices=("uart", "wireless"), default="uart")
    parser.add_argument(
        "--provider",
        choices=("codex", "claude-code", "openai", "openrouter", "anthropic"),
    )
    parser.add_argument("--model")
    parser.add_argument("--prompt", help="submit one prompt after the board agent is ready")
    parser.add_argument("--provision-wifi", action="store_true")
    args = parser.parse_args()
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    if args.backend == "uart" and provider not in ("codex", "claude-code"):
        parser.error("UART provider must be codex or claude-code")
    if args.backend == "wireless" and provider not in ("openai", "openrouter", "anthropic"):
        parser.error("wireless provider must be openai, openrouter or anthropic")
    runtime_config = config(args)
    model_backend = create_backend(provider) if args.backend == "uart" else None

    try:
        subprocess.run(
            ["espflash", "reset", "--port", args.port, "--non-interactive"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError:
        pass

    fd = os.open(args.port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    tty.setraw(fd)
    attributes = termios.tcgetattr(fd)
    attributes[4] = termios.B115200
    attributes[5] = termios.B115200
    termios.tcsetattr(fd, termios.TCSANOW, attributes)
    def stop(_signum: int, _frame: object) -> None:
        raise KeyboardInterrupt

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    pending = b""
    print(f"Pocket Pi UART bridge ready: {provider} on {args.port}", flush=True)
    try:
        while True:
            readable, _, _ = select.select([fd], [], [], 0.2)
            if not readable:
                continue
            pending += os.read(fd, 4096)
            while b"\n" in pending:
                raw, pending = pending.split(b"\n", 1)
                line = raw.decode(errors="replace").strip()
                if line.endswith(CONFIG_REQUEST):
                    frame = CONFIG_RESPONSE + json.dumps(runtime_config, separators=(",", ":"))
                    for _ in range(3):
                        time.sleep(0.1)
                        write_line(fd, frame)
                    print("[bridge] boot configuration sent", flush=True)
                elif line.endswith(WAITING):
                    write_line(fd, READY)
                elif line.startswith(REQUEST):
                    try:
                        if model_backend is None:
                            raise RuntimeError("wireless mode does not provide a UART model backend")
                        stream_stats = [0, 0]

                        def emit_delta(delta: str) -> None:
                            stream_stats[0] += 1
                            stream_stats[1] += len(delta)
                            write_line(
                                fd,
                                STREAM
                                + json.dumps(
                                    {"type": "text_delta", "text": delta},
                                    separators=(",", ":"),
                                ),
                            )

                        result = model_backend.complete(
                            json.loads(line[len(REQUEST) :]),
                            emit_delta,
                        )
                        call = result.get("toolCall")
                        if isinstance(call, dict):
                            print(
                                f"[bridge] ESP tool {call.get('name')}: "
                                + json.dumps(call.get("arguments", {}), ensure_ascii=False),
                                flush=True,
                            )
                        elif isinstance(result.get("text"), str):
                            print(f"[bridge] Pi reply: {result['text']}", flush=True)
                            print(
                                f"[bridge] streamed {stream_stats[0]} chunks / {stream_stats[1]} chars",
                                flush=True,
                            )
                        write_line(
                            fd,
                            STREAM
                            + json.dumps({"type": "done", "result": result}, separators=(",", ":")),
                        )
                    except Exception as error:
                        write_line(
                            fd,
                            STREAM
                            + json.dumps({"type": "error", "message": str(error)}, separators=(",", ":")),
                        )
                elif line:
                    print(line, flush=True)
    except (KeyboardInterrupt, OSError):
        return 0
    finally:
        if model_backend is not None:
            model_backend.close()
        os.close(fd)


if __name__ == "__main__":
    raise SystemExit(main())
