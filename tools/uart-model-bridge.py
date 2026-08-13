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
ROBINHOOD_KEYCHAIN_SERVICE = "Codex MCP Credentials"
ROBINHOOD_KEYCHAIN_ACCOUNT = "robinhood-trading|5cbe81c78ff5ae58"
EXA_KEYCHAIN_SERVICE = "Pocket Pi Credentials"
EXA_KEYCHAIN_ACCOUNT = "exa-api-key"
DEEPSEEK_KEYCHAIN_SERVICE = "Pocket Pi Credentials"
DEEPSEEK_KEYCHAIN_ACCOUNT = "deepseek-api-key"


def keychain_secret(service: str, account: str) -> str | None:
    """Read one generic-password value without ever printing it."""
    try:
        result = subprocess.run(
            ["security", "find-generic-password", "-s", service, "-a", account, "-w"],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None
    value = result.stdout.strip()
    return value if result.returncode == 0 and value else None


def robinhood_access_token() -> str | None:
    """Reuse a completed Codex MCP OAuth grant without exposing it to disk."""
    try:
        result = subprocess.run(
            [
                "security",
                "find-generic-password",
                "-s",
                ROBINHOOD_KEYCHAIN_SERVICE,
                "-a",
                ROBINHOOD_KEYCHAIN_ACCOUNT,
                "-w",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None
    if result.returncode != 0:
        return None
    try:
        credential = json.loads(result.stdout)
        token = credential.get("token_response", {}).get("access_token")
        return token if isinstance(token, str) and token else None
    except (json.JSONDecodeError, AttributeError):
        return None


def exa_api_key() -> str | None:
    """Load the user-provided Exa key from macOS Keychain."""
    return keychain_secret(EXA_KEYCHAIN_SERVICE, EXA_KEYCHAIN_ACCOUNT)


def deepseek_api_key() -> str | None:
    """Load the user-provided DeepSeek key from macOS Keychain."""
    return keychain_secret(DEEPSEEK_KEYCHAIN_SERVICE, DEEPSEEK_KEYCHAIN_ACCOUNT)


def write_line(fd: int, line: str) -> None:
    payload = memoryview(f"{line}\r\n".encode())
    while payload:
        try:
            count = os.write(fd, payload)
        except BlockingIOError:
            select.select([], [fd], [], 0.5)
            continue
        payload = payload[count:]


def log_tool_failure(request: dict[str, object]) -> None:
    """Print only failed ESP tool text, never successful account payloads."""
    context = request.get("context")
    if not isinstance(context, dict):
        return
    messages = context.get("messages")
    if not isinstance(messages, list):
        return
    for message in reversed(messages):
        if not isinstance(message, dict) or message.get("role") != "toolResult":
            continue
        if message.get("isError") is not True:
            return
        content = message.get("content")
        if not isinstance(content, list):
            return
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                failure = str(item.get("text", "App tool failed"))[:400]
                print(f"[bridge] ESP tool failure: {failure}", flush=True)
                return


def config(args: argparse.Namespace) -> dict[str, object]:
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    value: dict[str, object] = {
        "modelBackend": args.backend,
        "modelProvider": provider,
        "unixTimeSeconds": int(time.time()),
    }
    if args.model:
        value["model"] = args.model
    value["thinkingLevel"] = args.thinking_level
    if args.provision_wifi:
        value["wifiSsid"] = input("Wi-Fi SSID: ").strip()
        value["wifiPassword"] = getpass.getpass("Wi-Fi password: ")
    if args.backend == "wireless":
        key = deepseek_api_key() if provider == "deepseek" else None
        if key:
            value["modelApiKey"] = key
            print("DeepSeek: reusing Keychain API key (RAM only on device)", flush=True)
        else:
            value["modelApiKey"] = getpass.getpass(f"{provider} API key: ")
    app_credentials: dict[str, str] = {}
    key = exa_api_key()
    if key:
        app_credentials["exa.api-key"] = key
        print("Exa: reusing Keychain API key (RAM only on device)", flush=True)
    elif args.provision_exa:
        key = getpass.getpass("Exa API key: ")
        if key:
            app_credentials["exa.api-key"] = key
    token = robinhood_access_token()
    if token:
        app_credentials["robinhood.oauth-access-token"] = token
        print(
            "Robinhood: reusing existing authorized Codex MCP session (RAM only)",
            flush=True,
        )
    elif args.provision_robinhood:
        token = getpass.getpass("Robinhood OAuth access token: ")
        if token:
            app_credentials["robinhood.oauth-access-token"] = token
    if app_credentials:
        value["appCredentials"] = app_credentials
    if args.prompt:
        value["initialPrompt"] = args.prompt
        if args.prompt_delay_seconds:
            value["initialPromptDelaySeconds"] = args.prompt_delay_seconds
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument("--backend", choices=("uart", "wireless"), default="uart")
    parser.add_argument(
        "--provider",
        choices=("codex", "claude-code", "openai", "openrouter", "anthropic", "deepseek"),
    )
    parser.add_argument("--model")
    parser.add_argument("--thinking-level", choices=("high", "xhigh"), default="high")
    parser.add_argument("--prompt", help="submit one prompt after the board agent is ready")
    parser.add_argument(
        "--prompt-delay-seconds",
        type=int,
        choices=range(0, 121),
        default=0,
        metavar="0..120",
        help="delay the repeatable boot prompt while device services settle",
    )
    parser.add_argument("--provision-wifi", action="store_true")
    parser.add_argument("--provision-exa", action="store_true")
    parser.add_argument("--provision-robinhood", action="store_true")
    args = parser.parse_args()
    provider = args.provider or ("codex" if args.backend == "uart" else "openai")
    if args.backend == "uart" and provider not in ("codex", "claude-code"):
        parser.error("UART provider must be codex or claude-code")
    if args.backend == "wireless" and provider not in ("openai", "openrouter", "anthropic", "deepseek"):
        parser.error("wireless provider must be openai, openrouter, anthropic or deepseek")
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
                        request_payload = json.loads(line[len(REQUEST) :])
                        log_tool_failure(request_payload)

                        def emit_delta(delta: str) -> None:
                            stream_stats[0] += 1
                            stream_stats[1] += len(delta)

                        result = model_backend.complete(
                            request_payload,
                            emit_delta,
                        )
                        calls = result.get("toolCalls")
                        if isinstance(calls, list) and calls:
                            for call in calls:
                                print(
                                    f"[bridge] ESP tool {call.get('name')}: "
                                    + json.dumps(call.get("arguments", {}), ensure_ascii=False),
                                    flush=True,
                                )
                        elif isinstance(result.get("text"), str):
                            print(f"[bridge] Pi reply: {result['text']}", flush=True)
                            print(
                                f"[bridge] coalesced {stream_stats[0]} provider chunks / "
                                f"{stream_stats[1]} chars into one UART result",
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
