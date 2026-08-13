"""Streaming Coding Plan backends used by the UART development transport."""

from __future__ import annotations

import json
import os
import re
import select
import subprocess
import tempfile
import time
from collections.abc import Callable

DeltaSink = Callable[[str], None]


def decision_prompt(request: dict[str, object]) -> tuple[str, list[object]]:
    context = request.get("context") if isinstance(request.get("context"), dict) else {}
    messages = context.get("messages") if isinstance(context.get("messages"), list) else []
    tools = context.get("tools") if isinstance(context.get("tools"), list) else []
    system = str(context.get("systemPrompt", ""))
    prompt = "\n\n".join(
        part
        for part in (
            "You are the model decision backend for a Pi Agent running on an ESP32-P4.",
            "Do not call Mac tools. The registered tools below run only on the ESP32.",
            "Return exactly one compact JSON object and no Markdown: "
            '{"toolCalls":[{"name":"registered.name","arguments":{...}}]} for actions, or '
            '{"text":"final response"} when the turn is complete. Never claim success before '
            "the corresponding tool result appears in the conversation.",
            f"System instruction: {system}" if system else "",
            "Registered ESP32 tools: "
            + json.dumps(tools, ensure_ascii=False, separators=(",", ":")),
            "Conversation: "
            + json.dumps(messages[-24:], ensure_ascii=False, separators=(",", ":")),
        )
        if part
    )
    return prompt, tools


def parse_decision(raw: str, tools: list[object], provider: str) -> dict[str, object]:
    raw = raw.strip()
    if raw.startswith("```"):
        raw = re.sub(r"^```(?:json)?\s*|\s*```$", "", raw, flags=re.IGNORECASE)
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        raise ValueError(f"{provider} returned invalid decision JSON: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{provider} decision must be a JSON object")
    calls = value.get("toolCalls")
    if isinstance(calls, list) and calls:
        registered = {
            tool.get("name")
            for tool in tools
            if isinstance(tool, dict) and isinstance(tool.get("name"), str)
        }
        result_calls: list[dict[str, object]] = []
        base_id = int(time.time() * 1000)
        for index, call in enumerate(calls):
            if not isinstance(call, dict):
                raise ValueError(f"{provider} toolCalls entries must be objects")
            name = call.get("name")
            arguments = call.get("arguments", {})
            if name not in registered:
                raise ValueError(f"{provider} requested unregistered ESP32 tool: {name}")
            if not isinstance(arguments, dict):
                raise ValueError("toolCalls arguments must be JSON objects")
            result_calls.append({
                "id": f"esp_{base_id}_{index}",
                "name": name,
                "arguments": arguments,
            })
        return {
            "thinking": "",
            "text": "",
            "toolCalls": result_calls,
            "usage": {},
            "stopReason": "toolUse",
        }
    if isinstance(value.get("text"), str):
        return {
            "thinking": "",
            "text": value["text"],
            "toolCalls": [],
            "usage": {},
            "stopReason": "stop",
        }
    raise ValueError(f"{provider} decision needs text or non-empty toolCalls")


def _partial_text(raw: str) -> str | None:
    """Decode the complete prefix of a top-level JSON text field."""
    match = re.search(r'"text"\s*:\s*"', raw)
    if match is None:
        return None
    source = raw[match.end() :]
    decoded: list[str] = []
    escapes = {
        '"': '"',
        "\\": "\\",
        "/": "/",
        "b": "\b",
        "f": "\f",
        "n": "\n",
        "r": "\r",
        "t": "\t",
    }
    index = 0
    while index < len(source):
        char = source[index]
        if char == '"':
            break
        if char != "\\":
            decoded.append(char)
            index += 1
            continue
        if index + 1 >= len(source):
            break
        escaped = source[index + 1]
        if escaped in escapes:
            decoded.append(escapes[escaped])
            index += 2
            continue
        if escaped == "u":
            digits = source[index + 2 : index + 6]
            if len(digits) != 4 or not all(char in "0123456789abcdefABCDEF" for char in digits):
                break
            decoded.append(chr(int(digits, 16)))
            index += 6
            continue
        break
    return "".join(decoded)


class _DecisionTextStream:
    def __init__(self, emit: DeltaSink) -> None:
        self.raw = ""
        self.emitted = ""
        self.emit = emit

    def feed(self, raw_delta: str) -> None:
        self.raw += raw_delta
        decoded = _partial_text(self.raw)
        if decoded is None or len(decoded) <= len(self.emitted):
            return
        delta = decoded[len(self.emitted) :]
        self.emitted = decoded
        self.emit(delta)


class CodexBackend:
    """Persistent Codex app-server transport using the logged-in Coding Plan."""

    def __init__(self) -> None:
        self.workspace = tempfile.TemporaryDirectory(prefix="pocket-pi-codex-")
        self.process = subprocess.Popen(
            ["codex", "app-server", "--stdio"],
            cwd=self.workspace.name,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
        )
        self.next_id = 1
        self.read_buffer = b""
        self._request(
            "initialize",
            {
                "clientInfo": {
                    "name": "pocket_pi_uart_bridge",
                    "title": "Pocket Pi UART Bridge",
                    "version": "0.1.0",
                }
            },
        )
        self._send({"method": "initialized", "params": {}})

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self.workspace.cleanup()

    def _send(self, message: dict[str, object]) -> None:
        if self.process.stdin is None or self.process.poll() is not None:
            raise RuntimeError("Codex app-server is not running")
        self.process.stdin.write(
            (json.dumps(message, separators=(",", ":")) + "\n").encode()
        )
        self.process.stdin.flush()

    def _read(self, timeout: float = 180.0) -> dict[str, object]:
        if self.process.stdout is None:
            raise RuntimeError("Codex app-server stdout is unavailable")
        deadline = time.monotonic() + timeout
        while b"\n" not in self.read_buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Codex app-server response timed out")
            readable, _, _ = select.select([self.process.stdout], [], [], remaining)
            if not readable:
                raise TimeoutError("Codex app-server response timed out")
            chunk = os.read(self.process.stdout.fileno(), 64 * 1024)
            if not chunk:
                raise RuntimeError(f"Codex app-server exited {self.process.poll()}")
            self.read_buffer += chunk
        line, self.read_buffer = self.read_buffer.split(b"\n", 1)
        value = json.loads(line.decode())
        if not isinstance(value, dict):
            raise ValueError("Codex app-server emitted a non-object message")
        return value

    def _request(
        self,
        method: str,
        params: dict[str, object],
        on_notification: Callable[[dict[str, object]], None] | None = None,
    ) -> dict[str, object]:
        request_id = self.next_id
        self.next_id += 1
        self._send({"method": method, "id": request_id, "params": params})
        while True:
            message = self._read()
            if message.get("id") == request_id:
                if message.get("error") is not None:
                    raise RuntimeError(f"Codex app-server {method}: {message['error']}")
                result = message.get("result")
                return result if isinstance(result, dict) else {}
            if on_notification is not None:
                on_notification(message)

    def complete(self, request: dict[str, object], emit: DeltaSink) -> dict[str, object]:
        prompt, tools = decision_prompt(request)
        started = self._request(
            "thread/start",
            {
                "cwd": self.workspace.name,
                "approvalPolicy": "never",
                "sandbox": "read-only",
                "ephemeral": True,
                "baseInstructions": "Do not use Mac tools. Return only the requested compact JSON decision.",
            },
        ).get("thread")
        if not isinstance(started, dict) or not isinstance(started.get("id"), str):
            raise RuntimeError("Codex thread/start omitted thread id")
        thread_id = started["id"]
        phases: dict[str, str | None] = {}
        streams: dict[str, _DecisionTextStream] = {}
        final_text: str | None = None
        turn_finished = False

        def notification(message: dict[str, object]) -> None:
            nonlocal final_text, turn_finished
            method = message.get("method")
            params = message.get("params") if isinstance(message.get("params"), dict) else {}
            if method == "item/started":
                item = params.get("item") if isinstance(params.get("item"), dict) else {}
                if item.get("type") == "agentMessage" and isinstance(item.get("id"), str):
                    item_id = item["id"]
                    phases[item_id] = item.get("phase") if isinstance(item.get("phase"), str) else None
                    streams[item_id] = _DecisionTextStream(emit)
            elif method == "item/agentMessage/delta":
                item_id = params.get("itemId")
                delta = params.get("delta")
                if isinstance(item_id, str) and isinstance(delta, str) and phases.get(item_id) != "commentary":
                    streams.setdefault(item_id, _DecisionTextStream(emit)).feed(delta)
            elif method == "item/completed":
                item = params.get("item") if isinstance(params.get("item"), dict) else {}
                if item.get("type") == "agentMessage" and isinstance(item.get("text"), str):
                    candidate = item["text"]
                    try:
                        parse_decision(candidate, tools, "Codex")
                    except ValueError:
                        pass
                    else:
                        final_text = candidate
            elif method == "turn/completed":
                turn = params.get("turn") if isinstance(params.get("turn"), dict) else {}
                if turn.get("status") not in (None, "completed"):
                    raise RuntimeError(f"Codex turn ended with status {turn.get('status')}")
                turn_finished = True

        self._request(
            "turn/start",
            {"threadId": thread_id, "input": [{"type": "text", "text": prompt}]},
            notification,
        )
        while not turn_finished:
            notification(self._read())
        self._request("thread/unsubscribe", {"threadId": thread_id})
        if final_text is None:
            raise ValueError("Codex turn returned no valid Pi decision")
        return parse_decision(final_text, tools, "Codex")


class ClaudeCodeBackend:
    def close(self) -> None:
        pass

    def complete(self, request: dict[str, object], emit: DeltaSink) -> dict[str, object]:
        prompt, tools = decision_prompt(request)
        stream = _DecisionTextStream(emit)
        with tempfile.TemporaryDirectory(prefix="pocket-pi-claude-") as workspace:
            process = subprocess.Popen(
                [
                    "claude",
                    "-p",
                    "--bare",
                    "--verbose",
                    "--output-format",
                    "stream-json",
                    "--include-partial-messages",
                    "--no-session-persistence",
                ],
                cwd=workspace,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
            )
            assert process.stdin is not None
            process.stdin.write(prompt)
            process.stdin.close()
            final_text: str | None = None
            assert process.stdout is not None
            for line in process.stdout:
                event = json.loads(line)
                if event.get("type") == "stream_event":
                    inner = event.get("event") if isinstance(event.get("event"), dict) else {}
                    delta = inner.get("delta") if isinstance(inner.get("delta"), dict) else {}
                    if inner.get("type") == "content_block_delta" and delta.get("type") == "text_delta":
                        text = delta.get("text")
                        if isinstance(text, str):
                            stream.feed(text)
                elif event.get("type") == "result" and isinstance(event.get("result"), str):
                    final_text = event["result"]
            stderr = process.stderr.read() if process.stderr is not None else ""
            return_code = process.wait()
        if return_code != 0:
            raise RuntimeError(f"claude exited {return_code}: {stderr[-1000:].strip()}")
        if final_text is None:
            raise ValueError("Claude Code response omitted its final result")
        return parse_decision(final_text, tools, "Claude Code")


def create_backend(provider: str):
    if provider == "codex":
        return CodexBackend()
    if provider == "claude-code":
        return ClaudeCodeBackend()
    raise ValueError(f"unsupported UART provider: {provider}")
