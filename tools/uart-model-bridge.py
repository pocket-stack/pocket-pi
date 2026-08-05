#!/usr/bin/env python3
"""Bridge ESP32 Pocket Pi model decisions to a logged-in Mac CLI."""

from __future__ import annotations

import argparse
import getpass
import json
import os
import re
import select
import signal
import subprocess
import sys
import tempfile
import termios
import time
import tty

CONFIG_REQUEST = "PPI-CONFIG-REQUEST"
CONFIG_RESPONSE = "PPI-CONFIG:"
READY = "PPI-RPC-READY"
WAITING = "PPI-RPC-WAITING"
REQUEST = "PPI-RPC-REQUEST:"
STREAM = "PPI-RPC-STREAM:"


def decision_prompt(request: dict[str, object]) -> tuple[str, set[str]]:
    context = request.get("context") if isinstance(request.get("context"), dict) else {}
    messages = context.get("messages") if isinstance(context.get("messages"), list) else []
    tools = context.get("tools") if isinstance(context.get("tools"), list) else []
    names = {
        str(tool["name"])
        for tool in tools
        if isinstance(tool, dict) and isinstance(tool.get("name"), str)
    }
    prompt = "\n\n".join(
        (
            "You are the model backend for a Pi Agent running on an ESP32-P4.",
            "Do not use Mac tools. Return exactly one JSON object and no Markdown: "
            '{"toolCall":{"name":"registered.name","arguments":{...}}} or '
            '{"text":"final response"}.',
            "Registered ESP32 tools: "
            + json.dumps(tools, ensure_ascii=False, separators=(",", ":")),
            "Conversation: "
            + json.dumps(messages[-24:], ensure_ascii=False, separators=(",", ":")),
        )
    )
    return prompt, names


def parse_decision(raw: str, registered: set[str]) -> dict[str, object]:
    raw = raw.strip()
    if raw.startswith("```"):
        raw = re.sub(r"^```(?:json)?\s*|\s*```$", "", raw, flags=re.IGNORECASE)
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("model response must be a JSON object")
    call = value.get("toolCall")
    if isinstance(call, dict):
        name = call.get("name")
        arguments = call.get("arguments", {})
        if name not in registered:
            raise ValueError(f"model requested unregistered tool: {name}")
        if not isinstance(arguments, dict):
            raise ValueError("tool arguments must be an object")
        return {
            "toolCall": {
                "id": f"esp_{int(time.time() * 1000)}",
                "name": name,
                "arguments": arguments,
            }
        }
    if isinstance(value.get("text"), str):
        return {"text": value["text"]}
    raise ValueError("model response needs text or toolCall")


def run_model(provider: str, request: dict[str, object]) -> dict[str, object]:
    prompt, registered = decision_prompt(request)
    with tempfile.TemporaryDirectory(prefix="pocket-pi-uart-") as workspace:
        if provider == "codex":
            command = [
                "codex",
                "exec",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
                "--sandbox",
                "read-only",
                "--skip-git-repo-check",
                "--color",
                "never",
                "-C",
                workspace,
                "-",
            ]
        else:
            command = ["claude", "-p", "--output-format", "text", prompt]
        result = subprocess.run(
            command,
            input=prompt if provider == "codex" else None,
            text=True,
            capture_output=True,
            check=False,
            timeout=180,
        )
    if result.returncode != 0:
        raise RuntimeError(f"{provider} exited {result.returncode}: {result.stderr[-800:]}")
    return parse_decision(result.stdout, registered)


def write_line(fd: int, line: str) -> None:
    payload = memoryview(f"{line}\r\n".encode())
    while payload:
        try:
            count = os.write(fd, payload)
        except BlockingIOError:
            select.select([], [fd], [], 0.5)
            continue
        payload = payload[count:]


def config(args: argparse.Namespace) -> dict[str, str]:
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    value = {"modelBackend": args.backend, "modelProvider": provider}
    if args.model:
        value["model"] = args.model
    if args.provision_wifi:
        value["wifiSsid"] = input("Wi-Fi SSID: ").strip()
        value["wifiPassword"] = getpass.getpass("Wi-Fi password: ")
    if args.backend == "wireless":
        value["modelApiKey"] = getpass.getpass(f"{provider} API key: ")
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
    parser.add_argument("--provision-wifi", action="store_true")
    args = parser.parse_args()
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    if args.backend == "uart" and provider not in ("codex", "claude-code"):
        parser.error("UART provider must be codex or claude-code")
    if args.backend == "wireless" and provider not in ("openai", "openrouter", "anthropic"):
        parser.error("wireless provider must be openai, openrouter or anthropic")
    runtime_config = config(args)

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
                    write_line(fd, CONFIG_RESPONSE + json.dumps(runtime_config, separators=(",", ":")))
                elif line.endswith(WAITING):
                    write_line(fd, READY)
                elif line.startswith(REQUEST):
                    try:
                        result = run_model(provider, json.loads(line[len(REQUEST) :]))
                        if isinstance(result.get("text"), str):
                            write_line(
                                fd,
                                STREAM
                                + json.dumps(
                                    {"type": "text_delta", "text": result["text"]},
                                    separators=(",", ":"),
                                ),
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
        os.close(fd)


if __name__ == "__main__":
    raise SystemExit(main())
