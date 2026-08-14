#!/usr/bin/env python3
"""Persist Pocket Pi wireless model configuration over UART."""

from __future__ import annotations

import argparse
import getpass
import json
import subprocess
import time

from uart_io import close_port, open_port, read_lines, reset_device, write_line

CONFIG_REQUEST = "PPI-CONFIG-REQUEST"
CONFIG_RESPONSE = "PPI-CONFIG:"
CONFIG_STORED = "PPI-CONFIG-STORED"
DEEPSEEK_KEYCHAIN_SERVICE = "Pocket Pi Credentials"
DEEPSEEK_KEYCHAIN_ACCOUNT = "deepseek-api-key"


def keychain_secret(service: str, account: str) -> str | None:
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument(
        "--provider",
        choices=("openai", "openrouter", "anthropic", "deepseek"),
        default="deepseek",
    )
    parser.add_argument("--model")
    parser.add_argument("--thinking-level", choices=("high", "xhigh"), default="high")
    parser.add_argument("--provision-wifi", action="store_true")
    args = parser.parse_args()

    api_key = (
        keychain_secret(DEEPSEEK_KEYCHAIN_SERVICE, DEEPSEEK_KEYCHAIN_ACCOUNT)
        if args.provider == "deepseek"
        else None
    )
    if api_key is None:
        api_key = getpass.getpass(f"{args.provider} API key: ")
    config: dict[str, object] = {
        "modelBackend": "wireless",
        "modelProvider": args.provider,
        "modelApiKey": api_key,
        "thinkingLevel": args.thinking_level,
        "unixTimeSeconds": int(time.time()),
    }
    if args.model:
        config["model"] = args.model
    if args.provision_wifi:
        config["wifiSsid"] = input("Wi-Fi SSID: ").strip()
        config["wifiPassword"] = getpass.getpass("Wi-Fi password: ")

    reset_device(args.port)
    fd = open_port(args.port)
    pending = b""
    deadline = time.monotonic() + 20
    try:
        while time.monotonic() < deadline:
            lines, pending = read_lines(fd, pending, 0.2)
            for line in lines:
                if line.endswith(CONFIG_REQUEST):
                    frame = CONFIG_RESPONSE + json.dumps(config, separators=(",", ":"))
                    for _ in range(3):
                        time.sleep(0.1)
                        write_line(fd, frame)
                elif line.endswith(CONFIG_STORED):
                    print("Pocket Pi configuration stored", flush=True)
                    return 0
        print("Provisioning failed: device did not confirm storage", flush=True)
        return 1
    except (KeyboardInterrupt, OSError):
        return 1
    finally:
        close_port(fd)


if __name__ == "__main__":
    raise SystemExit(main())
