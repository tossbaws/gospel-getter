#!/usr/bin/env python3
"""Regenerate every shipped app icon from packaging/icon-source.png.

The one source is the Chi Rho Code LLC brand mark (the C2 Chi-Rho, a
stained-glass window), the same art the website's mark is built from (see
site/README.md, "Brand"). It is committed byte for byte as the Captain
approved it: Brand_Attempts/2026-10-05_brand_chi-rho_C2_circuit-reliquary_polish.png,
2048x2048.

Each size is one of two drawings of that mark:
- 48 px and up, the detailed mark: the source itself, moved so the window is
  centred in the frame, with everything outside the gold rim made
  transparent, then downsampled.
- Below 48 px, the flat-small glyph that tools/site/build_brand_icons.py
  draws (a bold gold glyph on two dark panes, no rim), drawn at 2048 and
  downsampled to each size on its own. The stained glass and the circuit
  traces turn to mush below 48 px; the site's favicon follows the same rule.

Usage, from anywhere (Python 3 and Pillow, nothing else):
    python3 packaging/generate_icons.py

Commit everything it writes together. It also records the source's and every
output's SHA-256 in packaging/icons.sha256 (`sha256sum -c` reads it from the
repository root), and tests/frontend/icons.test.mjs fails if a shipped icon
or the source no longer matches that record. Every file it writes is the
same, byte for byte, on a rerun.
"""

import hashlib
import io
import math
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "packaging/icon-source.png"
MANIFEST = "packaging/icons.sha256"

# Importing the site's script runs nothing (it has a __main__ guard); keep
# Python from leaving a __pycache__ beside it.
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tools/site"))
import build_brand_icons as brand  # noqa: E402

W = brand.W
SMALLEST_DETAILED = 48

# The sizes Windows asks an app's .ico for: 16 to 40 for small icons, the
# title bar and the taskbar at 100% to 250% scale, 48 to 96 for the desktop
# and Alt-Tab, 256 for large icons and Start. A missing size is scaled from
# another, which blurs it.
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 256]

# Pillow's ICNS writer: the ic07 to ic14 PNG entries, at these pixel sizes.
ICNS_SIZES = [32, 64, 128, 256, 512, 1024]

PNGS = {
    "src-tauri/icons/32x32.png": 32,
    "src-tauri/icons/64x64.png": 64,
    "src-tauri/icons/128x128.png": 128,
    "src-tauri/icons/128x128@2x.png": 256,
    "src-tauri/icons/icon.png": 512,
    "packaging/icon.png": 512,
    # The webview's favicon, and About's icon (56 CSS px, so 112 at 2x).
    "ui/favicon.png": 128,
}

# The rim's outer edge is where the luminance falls through this level,
# halfway between the gold rim (about 200) and the black square (about 3).
EDGE_LEVEL = 100
# The cut sits this far inside that edge, so the antialiased pixels between
# gold and black, and the faint glow outside the rim, don't show as a dark
# fringe on a light background (2 px at 2048 is a quarter pixel at 256).
RIM_INSET = 2.0
ANGLES = 720
# The edge is measured at each angle and averaged over this many neighbours
# either side (5 degrees), which smooths the glass texture without rounding
# off the window's slight ellipse (958 to 965 px across its angles).
SMOOTHING = 10


def sample(px, x, y):
    """Bilinear luminance at (x, y), pixel centres on whole numbers."""
    x0, y0 = int(x), int(y)
    fx, fy = x - x0, y - y0
    top = px[x0, y0] * (1 - fx) + px[x0 + 1, y0] * fx
    bottom = px[x0, y0 + 1] * (1 - fx) + px[x0 + 1, y0 + 1] * fx
    return top * (1 - fy) + bottom * fy


def rim_edge(lum, cx, cy):
    """The rim's outer edge at each of ANGLES angles around (cx, cy): walks
    in from outside the glow until the luminance reaches EDGE_LEVEL."""
    px = lum.load()
    start = min(cx, cy, W - 2 - cx, W - 2 - cy) - 1
    step = 0.25
    radii = []
    for k in range(ANGLES):
        c, s = math.cos(2 * math.pi * k / ANGLES), math.sin(2 * math.pi * k / ANGLES)
        r = start
        outside = sample(px, cx + r * c, cy + r * s)
        while r > W / 4:
            inside = sample(px, cx + (r - step) * c, cy + (r - step) * s)
            if inside >= EDGE_LEVEL > outside:
                radii.append(r - step * (EDGE_LEVEL - outside) / (inside - outside))
                break
            r, outside = r - step, inside
        else:
            sys.exit(f"no rim found at {360 * k / ANGLES} degrees; check the source")
    return radii


def measure_rim(src):
    """The window's centre and its rim's smoothed outer radius per angle."""
    lum = src.convert("L")
    left, top, right, bottom = lum.point(lambda v: 255 if v > EDGE_LEVEL else 0).getbbox()
    cx, cy = (left + right - 1) / 2, (top + bottom - 1) / 2
    # With angles evenly spread, the edge points' mean is the centre of a
    # centred ellipse; two rounds settle it to well under a pixel.
    for _ in range(2):
        radii = rim_edge(lum, cx, cy)
        angles = [2 * math.pi * k / ANGLES for k in range(ANGLES)]
        cx += sum(r * math.cos(a) for r, a in zip(radii, angles)) / ANGLES
        cy += sum(r * math.sin(a) for r, a in zip(radii, angles)) / ANGLES
    radii = rim_edge(lum, cx, cy)
    window = 2 * SMOOTHING + 1
    smooth = [
        sum(radii[(k + d) % ANGLES] for d in range(-SMOOTHING, SMOOTHING + 1)) / window
        for k in range(ANGLES)
    ]
    return cx, cy, smooth


def detailed(src):
    """The detailed mark at W: the window moved (by whole pixels, so the
    art isn't resampled) to the frame's centre, transparent outside the rim
    with a one-pixel antialiased edge."""
    cx, cy, radii = measure_rim(src)
    centre = (W - 1) / 2
    dx, dy = round(centre - cx), round(centre - cy)
    cx, cy = cx + dx, cy + dy

    lo, hi = min(radii) - RIM_INSET - 1, max(radii) - RIM_INSET + 1
    alpha = bytearray(W * W)
    for y in range(W):
        ry = y - cy
        for x in range(W):
            rx = x - cx
            d2 = rx * rx + ry * ry
            if d2 <= lo * lo:
                alpha[y * W + x] = 255
            elif d2 < hi * hi:
                pos = (math.atan2(ry, rx) / (2 * math.pi) % 1) * ANGLES
                k = int(pos) % ANGLES
                frac = pos - int(pos)
                edge = radii[k] * (1 - frac) + radii[(k + 1) % ANGLES] * frac - RIM_INSET
                cover = min(max(edge - math.sqrt(d2) + 0.5, 0.0), 1.0)
                alpha[y * W + x] = round(255 * cover)

    mark = Image.new("RGB", (W, W))
    mark.paste(src, (dx, dy))
    mark = mark.convert("RGBA")
    mark.putalpha(Image.frombytes("L", (W, W), bytes(alpha)))
    print(
        f"rim: centre ({cx - dx:.2f}, {cy - dy:.2f}) in the source, moved by ({dx}, {dy}); "
        f"outer radius {min(radii):.1f} to {max(radii):.1f} px at {W}, cut {RIM_INSET} px inside"
    )
    return mark


def png_bytes(img):
    """8-bit RGBA, always: Tauri's codegen refuses any other PNG icon."""
    out = io.BytesIO()
    img.convert("RGBA").save(out, "PNG", optimize=True)
    return out.getvalue()


def main():
    src = Image.open(ROOT / SOURCE)
    if src.size != (W, W) or src.mode != "RGB":
        sys.exit(f"expected a {W}x{W} RGB source, got {src.size} {src.mode}")
    src.load()
    big = detailed(src)
    small = brand.flat_small(brand.segment(src))

    cache = {}

    def icon(size):
        if size not in cache:
            master = big if size >= SMALLEST_DETAILED else small
            # Pillow resizes RGBA with premultiplied alpha, so the
            # transparent black outside the rim doesn't bleed in.
            cache[size] = master.resize((size, size), Image.Resampling.LANCZOS)
        return cache[size]

    written = {}
    for path, size in PNGS.items():
        written[path] = png_bytes(icon(size))

    frames = [icon(s) for s in ICO_SIZES]
    out = io.BytesIO()
    # The largest frame is the one saved; Pillow leaves out any size above it.
    frames[-1].save(out, "ICO", sizes=[f.size for f in frames], append_images=frames[:-1])
    written["src-tauri/icons/icon.ico"] = out.getvalue()

    out = io.BytesIO()
    icns = [icon(s) for s in ICNS_SIZES]
    icns[-1].save(out, "ICNS", append_images=icns[:-1])
    written["src-tauri/icons/icon.icns"] = out.getvalue()

    for path, data in written.items():
        (ROOT / path).write_bytes(data)

    lines = [f"{hashlib.sha256((ROOT / SOURCE).read_bytes()).hexdigest()}  {SOURCE}"]
    lines += [f"{hashlib.sha256(data).hexdigest()}  {path}" for path, data in sorted(written.items())]
    (ROOT / MANIFEST).write_text("\n".join(lines) + "\n")

    for path, data in sorted(written.items()):
        print(f"{len(data):>8}  {path}")


if __name__ == "__main__":
    main()
