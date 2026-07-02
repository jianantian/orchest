#!/usr/bin/env python3
"""Deterministically generate fixtures/research/chart.png.

Draws a simple bar chart of referral-channel 30-day retention by quarter,
matching the numbers in `001-retention-dashboard-notes.md`. Pure Python
stdlib (struct + zlib) — no image library dependency, so it is safe to
re-run on any machine with Python 3 to regenerate the fixture byte-for-byte.

Usage: python3 gen_chart.py
Writes: ../research/chart.png
"""

import struct
import zlib
from pathlib import Path

WIDTH, HEIGHT = 320, 220
WHITE = (255, 255, 255)
BLACK = (0, 0, 0)
BAR_COLOR = (70, 130, 180)

# 5x7 dot-matrix font, only the glyphs this chart needs.
FONT = {
    "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
    "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    "2": ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
    "3": ["01110", "10001", "00001", "00110", "00001", "10001", "01110"],
    "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
    "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
    "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
    "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
    "9": ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
    "Q": ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
    "%": ["11001", "11010", "00010", "00100", "01000", "01011", "10011"],
    "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
    " ": ["00000"] * 7,
}


def new_canvas(width, height, color):
    row = bytes(color) * width
    return [bytearray(row) for _ in range(height)]


def set_pixel(canvas, x, y, color):
    if 0 <= x < len(canvas[0]) // 3 and 0 <= y < len(canvas):
        canvas[y][x * 3 : x * 3 + 3] = bytes(color)


def fill_rect(canvas, x0, y0, x1, y1, color):
    for y in range(y0, y1):
        for x in range(x0, x1):
            set_pixel(canvas, x, y, color)


def draw_text(canvas, x, y, text, color, scale=2):
    """Draw text with top-left corner at (x, y)."""
    cursor = x
    for ch in text:
        glyph = FONT.get(ch, FONT[" "])
        for row_idx, row in enumerate(glyph):
            for col_idx, bit in enumerate(row):
                if bit == "1":
                    fill_rect(
                        canvas,
                        cursor + col_idx * scale,
                        y + row_idx * scale,
                        cursor + col_idx * scale + scale,
                        y + row_idx * scale + scale,
                        color,
                    )
        cursor += (5 * scale) + scale  # glyph width + 1-unit spacing


def text_width(text, scale=2):
    return len(text) * ((5 * scale) + scale)


def write_png(path, canvas):
    width = len(canvas[0]) // 3
    height = len(canvas)

    def chunk(tag, data):
        out = struct.pack(">I", len(data)) + tag + data
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return out + struct.pack(">I", crc)

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)  # 8-bit RGB
    raw = bytearray()
    for row in canvas:
        raw.append(0)  # filter type 0 (none) per scanline
        raw.extend(row)
    idat = zlib.compress(bytes(raw), level=9)
    png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
    path.write_bytes(png)


def main():
    quarters = ["Q1", "Q2", "Q3"]
    values = [38, 40, 42]  # referral channel 30-day retention %, matches 001

    canvas = new_canvas(WIDTH, HEIGHT, WHITE)

    left, right, top, bottom = 40, 20, 20, 190
    plot_h = bottom - top
    max_value = 50

    # axes
    fill_rect(canvas, left, top, left + 1, bottom + 1, BLACK)
    fill_rect(canvas, left, bottom, WIDTH - right, bottom + 1, BLACK)

    bar_w, gap = 60, 20
    x = left + gap
    for label, value in zip(quarters, values):
        bar_h = int(plot_h * value / max_value)
        y0 = bottom - bar_h
        fill_rect(canvas, x, y0, x + bar_w, bottom, BAR_COLOR)

        value_label = f"{value}%"
        label_w = text_width(value_label, scale=2)
        draw_text(canvas, x + bar_w // 2 - label_w // 2, y0 - 18, value_label, BLACK, scale=2)

        axis_w = text_width(label, scale=2)
        draw_text(canvas, x + bar_w // 2 - axis_w // 2, bottom + 6, label, BLACK, scale=2)

        x += bar_w + gap

    out_path = Path(__file__).resolve().parent.parent / "research" / "chart.png"
    write_png(out_path, canvas)
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
