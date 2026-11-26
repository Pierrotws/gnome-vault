#!/usr/bin/env python3
"""Builds every icon from assets/logo.svg.

    python3 assets/icons/make-icons.py      # needs Pillow and rsvg-convert

Writes, from the repository root:
- assets/icons/hicolor/<N>x<N>/apps/io.pierrotws.Passel.png: Linux icons;
- assets/icons/passel.ico: the Windows icon (program, installer,
  shortcuts);
- android/app/src/main/res/mipmap-*/ic_launcher_foreground.png: the
  Android adaptive icon's foreground (its background is white);
- assets/android/store/icon.png and feature-graphic.png: the Google Play
  store listing's app icon and feature graphic (the text of the latter is
  in feature-graphic.svg). Uploaded by hand in Play Console, Grow users >
  Store presence > Store listings, with the screenshots next to them.
"""

import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SVG = ROOT / "assets/logo.svg"
APP_ID = "io.pierrotws.Passel"
# The SVG is rasterized once, at this size, then scaled down.
RENDER_SIZE = 1024
LINUX_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
# Explorer, the taskbar and the title bar pick from these.
WINDOWS_SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
# Android adaptive icon: a 108 dp layer, the logo drawn in its central
# 48 dp: the launcher's mask (circle, squircle…) never cuts it, and it
# keeps the margin other launcher icons have.
ANDROID_DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
ANDROID_LAYER_DP, ANDROID_CONTENT_DP = 108, 48
# Of the 108 dp layer, launchers show the central 72 dp.
ANDROID_VISIBLE_DP = 72
STORE = ROOT / "assets/android/store"
PLAY_ICON_SIZE = 512
# Where the logo goes in feature-graphic.svg: left, top, side.
LOGO_BOX = (72, 90, 320)


def render(width: int, height: int | None = None, svg: Path = SVG) -> Image.Image:
    """Rasterizes `svg` to `width`x`height` (square by default)."""
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "logo.png"
        subprocess.run(
            ["rsvg-convert", "-w", str(width), "-h", str(height or width), "-o", str(png), str(svg)],
            check=True,
        )
        return Image.open(png).convert("RGBA")


def trimmed_square(img: Image.Image, margin: float) -> Image.Image:
    """Crops to the visible logo plus `margin` (fraction of its size), squared."""
    left, top, right, bottom = img.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox()
    side = max(right - left, bottom - top)
    side = round(side * (1 + 2 * margin))
    cx, cy = (left + right) / 2, (top + bottom) / 2
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(img, (round(side / 2 - cx), round(side / 2 - cy)))
    return canvas


def main() -> None:
    clear = render(RENDER_SIZE)

    icon = trimmed_square(clear, margin=0.04)
    for size in LINUX_SIZES:
        dest = ROOT / f"assets/icons/hicolor/{size}x{size}/apps/{APP_ID}.png"
        dest.parent.mkdir(parents=True, exist_ok=True)
        icon.resize((size, size), Image.LANCZOS).save(dest, optimize=True)

    icon.resize((256, 256), Image.LANCZOS).save(
        ROOT / "assets/icons/passel.ico", sizes=[(s, s) for s in WINDOWS_SIZES]
    )

    content = trimmed_square(clear, margin=0)
    for density, scale in ANDROID_DENSITIES.items():
        layer = round(ANDROID_LAYER_DP * scale)
        inner = round(ANDROID_CONTENT_DP * scale)
        canvas = Image.new("RGBA", (layer, layer), (0, 0, 0, 0))
        offset = (layer - inner) // 2
        canvas.paste(content.resize((inner, inner), Image.LANCZOS), (offset, offset))
        dest = ROOT / f"android/app/src/main/res/mipmap-{density}/ic_launcher_foreground.png"
        dest.parent.mkdir(parents=True, exist_ok=True)
        canvas.save(dest, optimize=True)

    # The Play store listing's icon: a full-bleed 512 px square (Play rounds
    # its corners), the launcher icon as a phone shows it: the logo on white,
    # 48 dp of the visible 72 dp.
    play = Image.new("RGB", (PLAY_ICON_SIZE, PLAY_ICON_SIZE), "white")
    inner = round(PLAY_ICON_SIZE * ANDROID_CONTENT_DP / ANDROID_VISIBLE_DP)
    logo = content.resize((inner, inner), Image.LANCZOS)
    offset = (PLAY_ICON_SIZE - inner) // 2
    play.paste(logo, (offset, offset), logo)
    play.save(STORE / "icon.png", optimize=True)

    # Play's feature graphic: the background and text, then the logo.
    feature = render(1024, 500, STORE / "feature-graphic.svg").convert("RGB")
    left, top, side = LOGO_BOX
    logo = content.resize((side, side), Image.LANCZOS)
    feature.paste(logo, (left, top), logo)
    feature.save(STORE / "feature-graphic.png", optimize=True)


if __name__ == "__main__":
    main()
