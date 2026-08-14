"""Small shared helpers for Pocket Pi UART command-line tools."""

from __future__ import annotations

import fcntl
import os
import select
import struct
import subprocess
import termios
import tty


def open_port(path: str) -> int:
    fd = os.open(path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    tty.setraw(fd)
    attributes = termios.tcgetattr(fd)
    attributes[4] = termios.B115200
    attributes[5] = termios.B115200
    termios.tcsetattr(fd, termios.TCSANOW, attributes)
    return fd


def close_port(fd: int) -> None:
    try:
        termios.tcdrain(fd)
        attributes = termios.tcgetattr(fd)
        attributes[2] &= ~termios.HUPCL
        attributes[2] |= termios.CLOCAL
        termios.tcsetattr(fd, termios.TCSANOW, attributes)
        lines = struct.pack("I", termios.TIOCM_DTR | termios.TIOCM_RTS)
        fcntl.ioctl(fd, termios.TIOCMBIC, lines)
    finally:
        os.close(fd)


def reset_device(port: str) -> None:
    try:
        subprocess.run(
            ["espflash", "reset", "--port", port, "--non-interactive"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError:
        pass


def write_line(fd: int, line: str) -> None:
    payload = memoryview(f"{line}\r\n".encode())
    while payload:
        try:
            count = os.write(fd, payload)
        except BlockingIOError:
            select.select([], [fd], [], 0.5)
            continue
        payload = payload[count:]


def read_lines(fd: int, pending: bytes, timeout: float) -> tuple[list[str], bytes]:
    readable, _, _ = select.select([fd], [], [], timeout)
    if readable:
        pending += os.read(fd, 4096)
    lines: list[str] = []
    while b"\n" in pending:
        raw, pending = pending.split(b"\n", 1)
        line = raw.decode(errors="replace").strip()
        if line:
            lines.append(line)
    return lines, pending
