"""Draws the MyDocDesk app icon (standard library only) and writes
assets/icon.png (256x256, used as the window icon) and assets/icon.ico
(16..256, embedded in the .exe and used by the installer).

Run from app/:  python packaging/make_icon.py
"""
import os
import struct
import zlib

ACCENT = (0x1F, 0x5B, 0xD1)
PAGE = (0xFF, 0xFF, 0xFF)
LINE = (0xB9, 0xCB, 0xF0)
FOLD = (0xD6, 0xE1, 0xF7)
SS = 4  # supersampling per axis


def rounded_rect(x, y, x0, y0, x1, y1, r):
    if x < x0 or x > x1 or y < y0 or y > y1:
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def shape(u, v):
    """Colour at (u, v) in 0..1 coordinates, or None (transparent)."""
    if not rounded_rect(u, v, 0.02, 0.02, 0.98, 0.98, 0.22):
        return None
    # the page, with its top-right corner folded
    px0, py0, px1, py1, fold = 0.27, 0.18, 0.73, 0.82, 0.16
    if px0 <= u <= px1 and py0 <= v <= py1:
        if u > px1 - fold and v < py0 + fold:
            dx, dy = u - (px1 - fold), v - py0
            if dx > dy:
                return ACCENT  # cut away
            return FOLD
        for ly, lx1 in ((0.42, 0.63), (0.52, 0.63), (0.62, 0.63), (0.72, 0.52)):
            if abs(v - ly) <= 0.022 and 0.35 <= u <= lx1:
                return LINE
        return PAGE
    return ACCENT


def render(n):
    rows = []
    for y in range(n):
        row = bytearray([0])  # PNG filter: none
        for x in range(n):
            acc = [0, 0, 0, 0]
            for sy in range(SS):
                for sx in range(SS):
                    c = shape((x + (sx + 0.5) / SS) / n, (y + (sy + 0.5) / SS) / n)
                    if c is not None:
                        acc[0] += c[0]
                        acc[1] += c[1]
                        acc[2] += c[2]
                        acc[3] += 1
            k = acc[3]
            if k == 0:
                row += bytes(4)
            else:
                row += bytes([acc[0] // k, acc[1] // k, acc[2] // k, 255 * k // (SS * SS)])
        rows.append(bytes(row))
    return png(n, b"".join(rows))


def png(n, raw):
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico(images):
    head = struct.pack("<HHH", 0, 1, len(images))
    off = 6 + 16 * len(images)
    dirs, data = b"", b""
    for n, p in images:
        dirs += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(p), off + len(data))
        data += p
    return head + dirs + data


def main():
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
    os.makedirs(out, exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [(n, render(n)) for n in sizes]
    with open(os.path.join(out, "icon.png"), "wb") as f:
        f.write(images[-1][1])
    with open(os.path.join(out, "icon.ico"), "wb") as f:
        f.write(ico(images))
    print("wrote assets/icon.png, assets/icon.ico")


if __name__ == "__main__":
    main()
