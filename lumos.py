#! /usr/bin/env python3

"""Terminal background color detection utility.

This module queries the terminal for its background color,
and determines whether it's a dark or light theme.
"""

from __future__ import annotations

import os
import re
import select
import sys
import termios

DEBUG = os.environ.get("DEBUG") is not None

DARK_THRESHOLD = 0.5


def debug(*args: str) -> None:
    """Print debug messages to stderr if DEBUG is enabled."""
    if DEBUG:
        print(*args, file=sys.stderr)  # noqa: T201


def query_bg_from_terminal() -> str | None:
    """Query the terminal for its background color using OSC 11.

    Returns:
        The background color string returned by the terminal,
        or None if the query fails or times out.

    """
    try:
        fd = os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)
    except OSError:
        return None
    old = termios.tcgetattr(fd)
    new = old[:]
    new[3] &= ~(termios.ICANON | termios.ECHO)
    termios.tcsetattr(fd, termios.TCSANOW, new)
    try:
        os.write(fd, b"\033]11;?\a")  # OSC 11 query
        buf = b""
        for _ in range(100):  # up to ~2s
            r, _, _ = select.select([fd], [], [], 0.02)
            if not r:
                continue
            buf += os.read(fd, 4096)
            if b"\a" in buf or b"\033\\" in buf:  # BEL or ST terminator
                break
    finally:
        termios.tcsetattr(fd, termios.TCSANOW, old)
        os.close(fd)
    m = re.search(rb"\]\s*11;([^\a\x1b]*)", buf)
    return m.group(1).decode("ascii", "ignore") if m else None


def parse_rgb(s: str) -> tuple[int, int, int] | None:
    """Parse an RGB color string into RGB tuple.

    Args:
        s: Color string in various formats (rgb:, rgba:, #hex, rgb())

    Returns:
        RGB tuple (r, g, b) with values 0-255, or None if parsing fails.

    """
    s = s.strip()
    if s.startswith(("rgb:", "rgba:")):
        parts = s.split(":", 1)[1].split("/")
        r, g, b = parts[:3]

        def h(x: str) -> int:
            """Convert hex string to integer, handling formats."""
            n = int(x, 16)
            hex_2_digit_max = 2
            return n if len(x) == hex_2_digit_max else round(n / 65535 * 255)

        return (h(r), h(g), h(b))
    if s.startswith("#") and len(s) in (7, 9):
        return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))
    m = re.match(r"rgb\((\d+),\s*(\d+),\s*(\d+)\)", s)
    if m:
        r, g, b = m.groups()
        return (int(r), int(g), int(b))
    return None


def luminance(rgb: tuple[int, int, int]) -> float:
    """Calculate relative luminance of RGB color using sRGB formula.

    Args:
        rgb: RGB tuple with values 0-255

    Returns:
        Relative luminance value between 0 and 1.

    """
    r, g, b = [c / 255.0 for c in rgb]

    def lin(c: float) -> float:
        """Convert sRGB component to linear RGB."""
        srgb_threshold = 0.04045
        return c / 12.92 if c <= srgb_threshold else ((c + 0.055) / 1.055) ** 2.4

    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)


reply = query_bg_from_terminal()
debug(f"{reply=}")

rgb = parse_rgb(reply) if reply else None
debug(f"{rgb=}")

if rgb:
    lum = luminance(rgb)
    debug(f"{lum=}")

    sys.stdout.write("dark" if lum < DARK_THRESHOLD else "light")
    sys.exit(0)

debug("unable to determine background color")
sys.stdout.write("unknown")
sys.exit(2)
