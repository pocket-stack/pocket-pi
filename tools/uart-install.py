#!/usr/bin/env python3
"""Upload one .pocketapp to a running Pocket Pi over UART."""

from __future__ import annotations

import argparse
import base64
from pathlib import Path
import time

from uart_io import close_port, open_port, read_lines, write_line

BEGIN = "PPI-INSTALL-BEGIN:"
CHUNK = "PPI-INSTALL-CHUNK:"
READY = "PPI-INSTALL-READY"
ACK = "PPI-INSTALL-ACK"
UPLOADED = "PPI-INSTALL-UPLOADED"
ERROR = "PPI-INSTALL-ERROR:"
CHUNK_BYTES = 3 * 1024


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port")
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    payload = args.package.read_bytes()

    fd = open_port(args.port)
    pending = b""
    started = False
    sent = 0
    next_begin = 0.0
    last_progress = time.monotonic()
    try:
        while time.monotonic() - last_progress < 15:
            if not started and time.monotonic() >= next_begin:
                write_line(fd, f"{BEGIN}{len(payload)}")
                next_begin = time.monotonic() + 1
            lines, pending = read_lines(fd, pending, 0.2)
            for line in lines:
                if line.endswith(READY):
                    started = True
                    chunk = payload[:CHUNK_BYTES]
                    write_line(fd, CHUNK + base64.b64encode(chunk).decode())
                    sent = len(chunk)
                    last_progress = time.monotonic()
                    print(f"Uploading {args.package.name} ({len(payload)} bytes)", flush=True)
                elif line.startswith(ACK) and started:
                    last_progress = time.monotonic()
                    if sent < len(payload):
                        chunk = payload[sent : sent + CHUNK_BYTES]
                        write_line(fd, CHUNK + base64.b64encode(chunk).decode())
                        sent += len(chunk)
                    else:
                        print("Package transferred; device is validating it", flush=True)
                elif line.startswith(UPLOADED):
                    print("Upload complete; confirm installation on Pocket Pi", flush=True)
                    print(
                        "Do not run espflash monitor before confirming; "
                        "it can reset the board and discard this review",
                        flush=True,
                    )
                    return 0
                elif line.startswith(ERROR):
                    print(f"Install failed: {line[len(ERROR):]}", flush=True)
                    return 1
        print("Install failed: device did not respond", flush=True)
        return 1
    except (KeyboardInterrupt, OSError):
        return 1
    finally:
        close_port(fd)


if __name__ == "__main__":
    raise SystemExit(main())
