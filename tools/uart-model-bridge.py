#!/usr/bin/env python3
"""Development-only bridge from Pocket Pi model requests to Codex or Claude Code."""

from __future__ import annotations

import argparse
import json
import signal
import time

from uart_bridge import create_backend
from uart_io import close_port, open_port, read_lines, reset_device, write_line

CONFIG_REQUEST = "PPI-CONFIG-REQUEST"
CONFIG_RESPONSE = "PPI-CONFIG:"
READY = "PPI-RPC-READY"
WAITING = "PPI-RPC-WAITING"
REQUEST = "PPI-RPC-REQUEST:"
STREAM = "PPI-RPC-STREAM:"


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
    value: dict[str, object] = {
        "modelBackend": "uart",
        "modelProvider": args.provider,
        "unixTimeSeconds": int(time.time()),
        "thinkingLevel": args.thinking_level,
    }
    if args.prompt:
        value["initialPrompt"] = args.prompt
        if args.prompt_delay_seconds:
            value["initialPromptDelaySeconds"] = args.prompt_delay_seconds
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument("--provider", choices=("codex", "claude-code"), default="codex")
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
    args = parser.parse_args()
    runtime_config = config(args)
    model_backend = create_backend(args.provider)

    reset_device(args.port)
    fd = open_port(args.port)

    def stop(_signum: int, _frame: object) -> None:
        raise KeyboardInterrupt

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    pending = b""
    print(f"Pocket Pi development model bridge ready: {args.provider} on {args.port}", flush=True)
    try:
        while True:
            lines, pending = read_lines(fd, pending, 0.2)
            for line in lines:
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
        model_backend.close()
        close_port(fd)


if __name__ == "__main__":
    raise SystemExit(main())
