#!/usr/bin/env python3
"""Render the artwork shown by the macOS and Windows installers.

The results in gui/src-tauri/installer/ are committed and read at bundle time,
so only run this after the app icon or the layout changes. It needs macOS for
tiffutil, Pillow, and a copy of Inter Medium (SIL Open Font License), which the
app names but does not ship:

    python3 scripts/release/render-installer-art.py --font /path/to/inter.woff

The mark is cut from the shipped icon rather than redrawn, so it cannot drift.

DMG_WINDOW, APP_ICON, and APPLICATIONS_ICON must match bundle.macOS.dmg in
tauri.conf.json. Finder draws both icons and their labels over this picture, and
picks dark labels for a light picture even in dark mode, so the background stays
light. NSIS stretches its bitmaps to fit the wizard with a basic GDI resampler,
so they are drawn at twice their nominal size to stay sharp at high display
scales, and in flat color because gradients would band.
"""

import argparse
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps

ROOT = Path(__file__).resolve().parents[2]
ICON = ROOT / "gui/src-tauri/icons/icon-macos-1024.png"
OUTPUT = ROOT / "gui/src-tauri/installer"

# Points. Finder's title bar takes about 30 from the window height, and the
# picture is anchored to the top left, so it runs past the visible area.
DMG_WINDOW = (660, 430)
DMG_PICTURE = (660, 420)
APP_ICON = (170, 152)
APPLICATIONS_ICON = (490, 152)
ICON_RADIUS = 64
CAPTION_Y = 318
CAPTION = "Drag Pipeline to Applications"

TOP = (252, 252, 252)
BOTTOM = (242, 242, 242)
ARROW = (150, 150, 150)
CAPTION_COLOR = (102, 102, 102)

# The app's --ui-inverse and --ui-on-inverse.
INK = (23, 23, 23)
PAPER = (255, 255, 255)

SUPERSAMPLE = 4


def mark() -> Image.Image:
    """Return the logo as a mask, trimmed to its bounds."""
    icon = Image.open(ICON).convert("RGBA")
    # This box sits inside the squircle, which excludes its hairline edge.
    inner = icon.crop((225, 165, 855, 840))
    # The squircle is not quite white, which would stamp a faint box.
    ink = ImageOps.invert(inner.convert("L")).point(lambda v: min(255, max(0, (v - 12) * 2)))
    return ink.crop(ink.getbbox())


def fitted_mark(height: int) -> Image.Image:
    source = mark()
    width = round(source.width * height / source.height)
    return source.resize((width, height), Image.LANCZOS)


def stamp(canvas: Image.Image, mask: Image.Image, at: tuple[int, int], color) -> None:
    canvas.paste(Image.new("RGB", mask.size, color), at, mask)


def vertical_gradient(size: tuple[int, int]) -> Image.Image:
    ramp = Image.new("RGB", (1, size[1]))
    for y in range(size[1]):
        t = y / max(size[1] - 1, 1)
        ramp.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM)))
    return ramp.resize(size)


def curve(start, control, end, steps=240):
    points = []
    for step in range(steps + 1):
        t = step / steps
        u = 1 - t
        points.append(tuple(u * u * s + 2 * u * t * c + t * t * e
                            for s, c, e in zip(start, control, end)))
    return points


def dmg_arrow(size: tuple[int, int], scale: int) -> Image.Image:
    """Draw the drag cue as one branch of the logo: a node, then an arc."""
    unit = scale * SUPERSAMPLE
    mask = Image.new("L", (size[0] * SUPERSAMPLE, size[1] * SUPERSAMPLE), 0)
    draw = ImageDraw.Draw(mask)
    y = APP_ICON[1]
    start = (APP_ICON[0] + ICON_RADIUS + 26, y - 2)
    end = (APPLICATIONS_ICON[0] - ICON_RADIUS - 26, y - 2)
    control = ((start[0] + end[0]) / 2, y - 30)
    path = [(px * unit, py * unit) for px, py in curve(start, control, end)]
    thickness = round(2.5 * unit)
    draw.line(path, fill=255, width=thickness, joint="curve")

    def dot(center, radius):
        x, y = center
        r = radius * unit
        draw.ellipse((x * unit - r, y * unit - r, x * unit + r, y * unit + r), fill=255)

    dot(start, 5)
    dot(end, 1.25)
    # Chevron along the arc's final tangent.
    tangent = (end[0] - control[0], end[1] - control[1])
    length = (tangent[0] ** 2 + tangent[1] ** 2) ** 0.5
    ux, uy = tangent[0] / length, tangent[1] / length
    for side in (-1, 1):
        wing = (end[0] + (-ux * 10 - side * uy * 7), end[1] + (-uy * 10 + side * ux * 7))
        draw.line([(end[0] * unit, end[1] * unit), (wing[0] * unit, wing[1] * unit)],
                  fill=255, width=thickness)
        dot(wing, 1.25)
    return mask.resize(size, Image.LANCZOS)


def dmg_background(scale: int, font_path: Path) -> Image.Image:
    size = (DMG_PICTURE[0] * scale, DMG_PICTURE[1] * scale)
    canvas = vertical_gradient(size)
    stamp(canvas, dmg_arrow(size, scale), (0, 0), ARROW)

    font = ImageFont.truetype(str(font_path), 13 * scale * SUPERSAMPLE)
    text = Image.new("L", (size[0] * SUPERSAMPLE, size[1] * SUPERSAMPLE), 0)
    width = font.getlength(CAPTION)
    x = (text.width - width) / 2
    ImageDraw.Draw(text).text((x, CAPTION_Y * scale * SUPERSAMPLE), CAPTION, fill=255,
                              font=font, anchor="ls")
    stamp(canvas, text.resize(size, Image.LANCZOS), (0, 0), CAPTION_COLOR)
    return canvas


def write_dmg_background(font_path: Path) -> None:
    with tempfile.TemporaryDirectory() as folder:
        low, high, joined = (Path(folder) / name for name in ("low.tiff", "high.tiff", "both.tiff"))
        dmg_background(1, font_path).save(low, dpi=(72, 72))
        dmg_background(2, font_path).save(high, dpi=(144, 144))
        subprocess.run(["tiffutil", "-cathidpicheck", low, high, "-out", joined], check=True,
                       capture_output=True)
        subprocess.run(["tiffutil", "-lzw", joined, "-out", OUTPUT / "dmg-background.tiff"],
                       check=True, capture_output=True)


def nsis_sidebar() -> Image.Image:
    """Nominally 164 x 314, drawn at twice that for high-resolution displays."""
    size = (328, 628)
    canvas = Image.new("RGB", size, INK)
    logo = fitted_mark(150)
    stamp(canvas, logo, ((size[0] - logo.width) // 2, 232), PAPER)
    return canvas


def nsis_header() -> Image.Image:
    """Nominally 150 x 57, shown at the right of a white page header."""
    size = (300, 114)
    canvas = Image.new("RGB", size, PAPER)
    small = fitted_mark(66)
    stamp(canvas, small, (size[0] - small.width - 28, (size[1] - small.height) // 2), INK)
    return canvas


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--font", type=Path, required=True, help="Inter Medium font file")
    font_path = parser.parse_args().font

    OUTPUT.mkdir(exist_ok=True)
    write_dmg_background(font_path)
    # Pillow writes RGB as the uncompressed 24-bit bitmap NSIS requires.
    nsis_sidebar().save(OUTPUT / "nsis-sidebar.bmp")
    nsis_header().save(OUTPUT / "nsis-header.bmp")


if __name__ == "__main__":
    main()
