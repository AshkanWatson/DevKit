#!/usr/bin/env python
"""Generate the DevKit application icon (deterministic, no AI image service).

Design: rounded-square dark canvas (#111315), a white upward arrow over
three rounded bars in the accent-blue family — the "version stack / switch
upward" motif from DESIGN.md (precise, calm, technical).

Output:
- apps/desktop/assets/icon/devkit_icon.png (1024x1024, source of truth)
- apps/desktop/windows/runner/resources/app_icon.ico (multi-size:
  16/24/32/48/64/128/256, used by the runner AND the Inno Setup installer)

Usage:  python scripts/generate_app_icon.py
"""
from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT_PNG = ROOT / "apps" / "desktop" / "assets" / "icon" / "devkit_icon.png"
OUT_ICO = ROOT / "apps" / "desktop" / "windows" / "runner" / "resources" / "app_icon.ico"
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]

SIZE = 1024
MARGIN = 16
RADIUS = 224
CANVAS = (17, 19, 21, 255)        # #111315 — DESIGN tokens backgroundDark
BORDER = (48, 54, 61, 255)        # #30363D — borderDark
ARROW = (243, 245, 247, 255)      # #F3F5F7 — textPrimaryDark
BAR_LIGHT = (164, 196, 255, 255)  # #A4C4FF
BAR_MID = (109, 155, 255, 255)    # #6D9BFF — accentDark
BAR_SOLID = (36, 107, 253, 255)   # #246BFD — accentLight


def rounded_square_mask() -> Image.Image:
    """Mask so every drawn element is clipped to the rounded square."""
    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (MARGIN, MARGIN, SIZE - MARGIN, SIZE - MARGIN), radius=RADIUS, fill=255
    )
    return mask


def main() -> None:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Dark canvas.
    d.rounded_rectangle(
        (MARGIN, MARGIN, SIZE - MARGIN, SIZE - MARGIN), radius=RADIUS, fill=CANVAS
    )

    cx = SIZE // 2

    # Upward arrow (version switching / upgrade).
    apex = (cx, 236)
    wing = 178
    wing_y = 448
    stem_half = 60
    stem_bottom = 556
    d.polygon(
        [
            apex,
            (cx - wing, wing_y),
            (cx - stem_half, wing_y),
            (cx - stem_half, stem_bottom),
            (cx + stem_half, stem_bottom),
            (cx + stem_half, wing_y),
            (cx + wing, wing_y),
        ],
        fill=ARROW,
    )

    # Version stack: narrowest/newest on top, widest below.
    bar_h = 84
    for color, y, half_width in [
        (BAR_LIGHT, 610, 272),
        (BAR_MID, 724, 348),
        (BAR_SOLID, 838, 420),
    ]:
        d.rounded_rectangle(
            (cx - half_width, y, cx + half_width, y + bar_h), radius=42, fill=color
        )

    # Clip everything to the rounded square.
    img.putalpha(rounded_square_mask())

    # Accent edge for definition at small sizes.
    overlay = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    od = ImageDraw.Draw(overlay)
    od.rounded_rectangle(
        (MARGIN, MARGIN, SIZE - MARGIN, SIZE - MARGIN), radius=RADIUS, outline=BORDER, width=6
    )
    img = Image.alpha_composite(img, overlay)

    OUT_PNG.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT_PNG, format="PNG")
    print(f"wrote {OUT_PNG}")

    OUT_ICO.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT_ICO, format="ICO", sizes=ICO_SIZES)
    print(f"wrote {OUT_ICO}")


if __name__ == "__main__":
    main()
