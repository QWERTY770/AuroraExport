#!/usr/bin/env python3
"""Generates the application icon (assets/icon.ico + assets/icon.png).

The icon mirrors the window icon drawn in `src/ui/mod.rs`: a rounded square
with a vertical gradient and a four pointed star.
"""
from __future__ import annotations

import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"

TOP = (0x53, 0x64, 0xC8)
BOTTOM = (0x9C, 0x4D, 0xE0)
STAR = (0xFF, 0xF3, 0xC4)

SUPERSAMPLE = 4


def render(size: int) -> Image.Image:
    big = size * SUPERSAMPLE
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    gradient = Image.new("RGBA", (big, big))
    pixels = gradient.load()
    for y in range(big):
        t = y / max(1, big - 1)
        color = tuple(
            int(TOP[i] + (BOTTOM[i] - TOP[i]) * t) for i in range(3)
        ) + (255,)
        for x in range(big):
            pixels[x, y] = color

    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, big - 1, big - 1], radius=int(big * 0.22), fill=255
    )
    image.paste(gradient, (0, 0), mask)

    # Four pointed star: |dx|^p + |dy|^p <= r
    star = Image.new("L", (big, big), 0)
    star_pixels = star.load()
    exponent = 0.55
    limit = 0.36
    for y in range(big):
        fy = (y + 0.5) / big - 0.5
        for x in range(big):
            fx = (x + 0.5) / big - 0.5
            value = abs(fx) ** exponent + abs(fy) ** exponent
            if value < limit:
                star_pixels[x, y] = 255
    image.paste(Image.new("RGBA", (big, big), STAR + (255,)), (0, 0), star)
    return image.resize((size, size), Image.LANCZOS)


def main() -> int:
    ASSETS.mkdir(parents=True, exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [render(size) for size in sizes]
    ico = ASSETS / "icon.ico"
    images[-1].save(ico, format="ICO", sizes=[(s, s) for s in sizes])
    images[-1].save(ASSETS / "icon.png")
    print(f"wrote {ico} ({ico.stat().st_size} bytes) and assets/icon.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
