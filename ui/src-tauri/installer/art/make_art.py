#!/usr/bin/env python3
"""Draws the installer bitmaps from the app icon and the design tokens.

    python3 ui/src-tauri/installer/art/make_art.py

Writes sidebar.bmp (welcome and finish pages) and header.bmp (every other
page) next to this file. Colours come from docs/DESIGN_SYSTEM.md. The
bitmaps are drawn at twice NSIS's nominal size (164x314, and 57x57 for the
header icon) because the pages use Rubik, whose dialog units are larger than
the stock font's, so NSIS scales them down rather than up.

Needs Pillow. Re-run after changing the icon or the palette and commit the
BMPs (the Tauri build only reads the BMPs).
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ICON = HERE.parents[1] / "icons" / "icon.png"
FONTS = HERE.parent / "fonts"

SCALE = 2  # output pixels per nominal pixel
SS = 4  # supersampling factor while drawing

PLUM_TOP = (0x2E, 0x1D, 0x45)
PLUM_BOTTOM = (0x1C, 0x12, 0x29)
TEXT_ON_PLUM = (0xF4, 0xEE, 0xFB)
ORANGE = (0xF0, 0x8A, 0x3C)
LILAC = (0xC9, 0xB6, 0xF2)
STRIP_TRACK = (0x3A, 0x27, 0x52)
GAMES = (0x8F, 0x6C, 0xE6)
SOCIAL = (0xE8, 0x70, 0x5C)
VIDEO = (0x2F, 0xA3, 0x83)
OTHER = (0x6E, 0x61, 0x85)
WHITE = (0xFF, 0xFF, 0xFF)


def icon_mark(size: int) -> Image.Image:
    """The icon's T-and-tether mark without its dark squircle."""
    src = Image.open(ICON).convert("RGBA")
    w, h = src.size
    # The mark sits inside the squircle; crop to it.
    box = (int(w * 0.30), int(h * 0.26), int(w * 0.74), int(h * 0.70))
    mark = src.crop(box)
    bg = src.getpixel((w // 2, int(h * 0.20)))[:3]
    px = mark.load()
    for y in range(mark.height):
        for x in range(mark.width):
            r, g, b, a = px[x, y]
            d = abs(r - bg[0]) + abs(g - bg[1]) + abs(b - bg[2])
            alpha = max(0, min(255, (d - 12) * 6))
            px[x, y] = (r, g, b, min(a, alpha))
    return mark.resize((size, size), Image.LANCZOS)


def gradient(w: int, h: int, top, bottom) -> Image.Image:
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return img


def sidebar() -> Image.Image:
    k = SCALE * SS
    W, H = 164 * k, 314 * k
    img = gradient(W, H, PLUM_TOP, PLUM_BOTTOM).convert("RGBA")
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)

    # Wordmark (the only text, so the bitmap works in every language).
    font = ImageFont.truetype(str(FONTS / "Unbounded-SemiBold.ttf"), 17 * k)
    d.text((18 * k, 24 * k), "Tether", font=font, fill=TEXT_ON_PLUM)

    # Ring: lilac track and an orange arc of time left, as on Today.
    cx, cy, r, sw = 82 * k, 150 * k, 50 * k, 9 * k
    bbox = (cx - r, cy - r, cx + r, cy + r)
    d.ellipse(bbox, outline=LILAC + (46,), width=sw)
    start, sweep = -90, 250
    d.arc(bbox, start, start + sweep, fill=ORANGE + (255,), width=sw)
    for ang in (start, start + sweep):  # round caps
        a = math.radians(ang)
        x = cx + (r - sw / 2) * math.cos(a)
        y = cy + (r - sw / 2) * math.sin(a)
        d.ellipse((x - sw / 2, y - sw / 2, x + sw / 2, y + sw / 2), fill=ORANGE + (255,))

    img = Image.alpha_composite(img, layer)
    mark = icon_mark(64 * k)
    img.alpha_composite(mark, (cx - mark.width // 2, cy - mark.height // 2))

    # Day strip: a day of use coloured by budget, with the orange "now" line.
    d = ImageDraw.Draw(img)
    x0, x1, y0, hgt = 18 * k, 146 * k, 262 * k, 12 * k
    d.rounded_rectangle((x0, y0, x1, y0 + hgt), radius=hgt // 2, fill=STRIP_TRACK)
    span = x1 - x0
    for a, b, col in [
        (0.10, 0.17, GAMES),
        (0.24, 0.27, OTHER),
        (0.33, 0.40, SOCIAL),
        (0.47, 0.55, VIDEO),
        (0.60, 0.63, OTHER),
        (0.66, 0.73, GAMES),
    ]:
        d.rounded_rectangle((x0 + a * span, y0 + 2 * k, x0 + b * span, y0 + hgt - 2 * k), radius=2 * k, fill=col)
    now = x0 + 0.78 * span
    d.rounded_rectangle((now - 1.5 * k, y0 - 5 * k, now + 1.5 * k, y0 + hgt + 5 * k), radius=1.5 * k, fill=ORANGE)

    return img.convert("RGB").resize((164 * SCALE, 314 * SCALE), Image.LANCZOS)


def header() -> Image.Image:
    """The app icon on white, square: NSIS fits it to the header's height."""
    k = SCALE * SS
    side = 57 * k
    img = Image.new("RGBA", (side, side), WHITE + (255,))
    icon = Image.open(ICON).convert("RGBA")
    size = 48 * k
    icon = icon.resize((size, size), Image.LANCZOS)
    img.alpha_composite(icon, ((side - size) // 2, (side - size) // 2))
    return img.convert("RGB").resize((57 * SCALE, 57 * SCALE), Image.LANCZOS)


if __name__ == "__main__":
    sidebar().save(HERE / "sidebar.bmp")
    header().save(HERE / "header.bmp")
    print("wrote", HERE / "sidebar.bmp", HERE / "header.bmp")
