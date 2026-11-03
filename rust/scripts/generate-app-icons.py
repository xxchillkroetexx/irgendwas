"""Generate the gift app icons with Python's standard library.

Coordinates use a 512px canvas. The gift stays inside the maskable safe circle
(radius 40% of the canvas), with an opaque background all the way to the edges.
Run from any directory; output is relative to this script.
"""

from pathlib import Path
import struct
import zlib

OUTPUT = Path(__file__).resolve().parent.parent / "public/assets/icons"
BACKGROUND = (220, 53, 69)
PAPER = (255, 255, 255)
RIBBON = (0, 145, 75)


def color(x, y):
    if ((204 <= x < 248 and 142 <= y < 166)
            or (264 <= x < 308 and 142 <= y < 166)
            or (192 <= x < 216 and 154 <= y < 192)
            or (296 <= x < 320 and 154 <= y < 192)
            or (204 <= x < 308 and 178 <= y < 202)):
        return PAPER
    if 150 <= x < 362 and 222 <= y < 366:
        return RIBBON if 242 <= x < 270 else PAPER
    if 138 <= x < 374 and 194 <= y < 234:
        return RIBBON if 242 <= x < 270 else PAPER
    return BACKGROUND


def chunk(kind, data):
    payload = kind + data
    return struct.pack("!I", len(data)) + payload + struct.pack("!I", zlib.crc32(payload))


def png(size):
    # Supersampling smooths edges at the smaller install-icon sizes.
    samples = 4
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            colors = [color((x + (sx + .5) / samples) * 512 / size,
                            (y + (sy + .5) / samples) * 512 / size)
                      for sy in range(samples) for sx in range(samples)]
            rows.extend(round(sum(c[i] for c in colors) / len(colors)) for i in range(3))
    header = struct.pack("!2I5B", size, size, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


if __name__ == "__main__":
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, size in [("icon-192", 192), ("icon-512", 512),
                       ("icon-maskable-512", 512), ("apple-touch-icon", 180)]:
        (OUTPUT / f"{name}.png").write_bytes(png(size))
