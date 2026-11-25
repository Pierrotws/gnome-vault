#!/usr/bin/env python3
"""Builds every icon from assets/logo.png (1080x1080, white background).

    python3 assets/icons/make-icons.py      # needs Pillow

Writes, from the repository root:
- assets/logo-transparent.png: the logo with its white background made
  transparent (the README and the Linux icons use it);
- assets/icons/hicolor/<N>x<N>/apps/io.pierrotws.Passel.png: Linux icons;
- android/app/src/main/res/mipmap-*/ic_launcher_foreground.png: the
  Android adaptive icon's foreground (its background is white, like the
  logo's).

Only the white connected to the image's border becomes transparent: the
white "P" and the dots inside the logo stay opaque. Near the edge of
that background, white is removed GIMP-style ("color to alpha"), so the
antialiased outline and the soft shadow fade out instead of leaving a
white fringe.
"""

from collections import deque
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
APP_ID = "io.pierrotws.Passel"
LINUX_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
# Android adaptive icon: a 108 dp layer, the logo drawn in its central
# 48 dp: the launcher's mask (circle, squircle…) never cuts it, and it
# keeps the margin other launcher icons have.
ANDROID_DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
ANDROID_LAYER_DP, ANDROID_CONTENT_DP = 108, 48

# A pixel this light belongs to the background if it touches it.
BACKGROUND_MIN = 232
# How far (px) around the background the edge is faded.
EDGE_PX = 3


def background_mask(img: Image.Image) -> Image.Image:
    """The light pixels connected to the border, as an L mask (255 = background)."""
    rgb = img.convert("RGB")
    w, h = rgb.size
    px = rgb.load()
    light = lambda x, y: min(px[x, y]) >= BACKGROUND_MIN  # noqa: E731
    seen = bytearray(w * h)
    queue = deque()
    for x in range(w):
        queue.extend([(x, 0), (x, h - 1)])
    for y in range(h):
        queue.extend([(0, y), (w - 1, y)])
    while queue:
        x, y = queue.popleft()
        i = y * w + x
        if seen[i] or not light(x, y):
            continue
        seen[i] = 1
        if x > 0:
            queue.append((x - 1, y))
        if x < w - 1:
            queue.append((x + 1, y))
        if y > 0:
            queue.append((x, y - 1))
        if y < h - 1:
            queue.append((x, y + 1))
    return Image.frombytes("L", (w, h), bytes(255 if s else 0 for s in seen))


def transparent(img: Image.Image) -> Image.Image:
    """Color-to-alpha against white, on the background and its edge only."""
    rgb = img.convert("RGB")
    zone = background_mask(rgb).filter(ImageFilter.MaxFilter(2 * EDGE_PX + 1))
    out = rgb.convert("RGBA")
    src, dst, mask = rgb.load(), out.load(), zone.load()
    w, h = rgb.size
    for y in range(h):
        for x in range(w):
            if not mask[x, y]:
                continue
            r, g, b = src[x, y]
            alpha = max(255 - r, 255 - g, 255 - b) / 255
            if alpha <= 0.02:
                dst[x, y] = (0, 0, 0, 0)
                continue
            # Un-blend from white: c = a * color + (1 - a) * 255.
            color = tuple(
                max(0, min(255, round((c - 255 * (1 - alpha)) / alpha))) for c in (r, g, b)
            )
            dst[x, y] = color + (round(alpha * 255),)
    return out


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
    logo = Image.open(ROOT / "assets/logo.png")
    clear = transparent(logo)
    clear.save(ROOT / "assets/logo-transparent.png", optimize=True)

    icon = trimmed_square(clear, margin=0.04)
    for size in LINUX_SIZES:
        dest = ROOT / f"assets/icons/hicolor/{size}x{size}/apps/{APP_ID}.png"
        dest.parent.mkdir(parents=True, exist_ok=True)
        icon.resize((size, size), Image.LANCZOS).save(dest, optimize=True)

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


if __name__ == "__main__":
    main()
