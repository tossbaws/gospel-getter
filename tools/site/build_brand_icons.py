#!/usr/bin/env python3
"""Build the flat small-size Chi Rho Code LLC icon set from the detailed mark.

The method follows the 2026-10-05 flat set
(/home/m4sk/Generated_Art/Brand_Attempts/favicon/2026-10-05_chi-rho_circuit_flat.json):
the gold Chi-Rho is color-segmented from the 2048px stained-glass mark
(r > 150, g > 130, b <= r + 5; a 5px close; flood-filled from the glyph;
a 9px close to remove the lead-line slivers). It is then redrawn as a solid
glyph on a flat two-tone circle and downsampled with Lanczos:

- flat: a #F2C14E glyph with a #1A140E outline, on emerald #108C48 (left) and
  cyan #12C8D6 (right) panes, with a gold rim and transparent corners.
- flat-small (16 and 32): a larger glyph emboldened with a 61px max filter at
  2048, in #FFCC54 on darker panes (#065C30 / #08787F), with no rim or
  outline.

The flood fill starts from two seeds, the centre of the X and a point on the
P's stem. In C the whole glyph is one piece, so the second seed changes
nothing. In C2 a lead line crosses the stem under the P's bowl, so a single
seed would lose the P. The glyph keeps its place in the 2048px frame: it is
scaled by a fixed factor about the frame's centre, which is also the icon
circle's centre. A change in the glyph's shape or position in the source
carries through to the icons. The geometry constants are measured from the
2026-10-05 set, which this script reproduces from C.

Usage, from the repository root (Pillow only):
    python3 tools/site/build_brand_icons.py SOURCE_PNG OUT_DIR [--site]

Writes OUT_DIR/2026-10-05_chi-rho_circuit_flat_{16,...,1024}.png,
..._flat-small_{16,32}.png and favicon.ico (16 and 32 flat-small, 48 and 64
flat). The names match the 2026-10-05 set; the folder says which source made
them. With --site, it also copies the sizes the site uses into
site/assets/brand/ under the site's names. Every PNG is optimized losslessly.
"""

import io
import shutil
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
SITE_BRAND = ROOT / "site/assets/brand"

W = 2048  # Working size; the source is 2048x2048 too.
PREFIX = "2026-10-05_chi-rho_circuit_"
SIZES = [16, 32, 48, 64, 128, 180, 256, 512, 1024]
SMALL_SIZES = [16, 32]

GLYPH = (0xF2, 0xC1, 0x4E)
OUTLINE = (0x1A, 0x14, 0x0E)
LEFT, RIGHT = (0x10, 0x8C, 0x48), (0x12, 0xC8, 0xD6)
SMALL_GOLD = (0xFF, 0xCC, 0x54)
SMALL_LEFT, SMALL_RIGHT = (0x06, 0x5C, 0x30), (0x08, 0x78, 0x7F)

# Geometry at 2048, measured from the 2026-10-05 set: the circle's outer
# edge, where the gold rim starts and where the panes start.
R_EDGE, R_RIM, R_PANES = 966, 938, 890
OUTLINE_WIDTH = 16
GLYPH_SCALE = 0.884  # flat glyph height / source glyph height
R_SMALL = 1000  # the flat-small circle
SMALL_GLYPH_SCALE = 1.07
SMALL_GLYPH_DROP = 24  # the flat-small glyph sits this much lower
SMALL_BOLD = 61  # max-filter size at 2048
P_SEED = -0.6  # the P's stem: this far above the centre, in source radii


def square(mask, size, grow):
    """A size x size square max (grow) or min filter, as 3x3 passes."""
    f = ImageFilter.MaxFilter(3) if grow else ImageFilter.MinFilter(3)
    for _ in range(size // 2):
        mask = mask.filter(f)
    return mask


def close(mask, size):
    return square(square(mask, size, True), size, False)


PLUS = ImageFilter.Kernel((3, 3), [0, 1, 0, 1, 1, 1, 0, 1, 0], scale=5)


def dilate_round(mask, radius):
    """Grow a mask by `radius`, alternating square and plus steps (an
    octagon, close to round)."""
    for i in range(radius):
        mask = mask.filter(ImageFilter.MaxFilter(3) if i % 2 else PLUS)
        mask = mask.point(lambda v: 255 if v else 0)
    return mask


def threshold(band, test):
    return band.point(lambda v: 255 if test(v) else 0).convert("1")


def segment(src):
    """The gold Chi-Rho, as an L mask."""
    r, g, b = src.split()
    gold = ImageChops.logical_and(threshold(r, lambda v: v > 150), threshold(g, lambda v: v > 130))
    gold = ImageChops.logical_and(gold, threshold(ImageChops.subtract(b, r), lambda v: v <= 5))
    gold = close(gold.convert("L"), 5)

    left, top, right, bottom = src.convert("L").point(lambda v: 255 if v > 40 else 0).getbbox()
    cx, cy, radius = (left + right) / 2, (top + bottom) / 2, (right - left) / 2

    glyph = gold.copy()
    for seed in [(cx, cy), (cx, cy + P_SEED * radius)]:
        seed = (round(seed[0]), round(seed[1]))
        if glyph.getpixel(seed) not in (255, 128):
            sys.exit(f"seed {seed} isn't on the gold glyph; check the source")
        ImageDraw.floodfill(glyph, seed, 128)
    glyph = close(glyph.point(lambda v: 255 if v == 128 else 0), 9)
    return glyph


def place(glyph, scale, drop=0):
    """The glyph scaled about the centre of the frame, `drop` lower."""
    c = W / 2
    data = (1 / scale, 0, c - c / scale, 0, 1 / scale, c - (c + drop) / scale)
    moved = glyph.transform((W, W), Image.Transform.AFFINE, data, Image.Resampling.BILINEAR)
    return moved.point(lambda v: 255 if v >= 128 else 0)


def disc(draw, radius, fill, halves=None):
    box = (W / 2 - radius, W / 2 - radius, W / 2 + radius - 1, W / 2 + radius - 1)
    if halves:
        draw.pieslice(box, 90, 270, fill=halves[0])
        draw.pieslice(box, 270, 90, fill=halves[1])
    else:
        draw.ellipse(box, fill=fill)


def flat(glyph):
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    disc(draw, R_EDGE, OUTLINE)
    disc(draw, R_RIM, GLYPH)
    disc(draw, R_PANES, None, (LEFT, RIGHT))
    g = place(glyph, GLYPH_SCALE)
    img.paste(OUTLINE, mask=dilate_round(g, OUTLINE_WIDTH))
    img.paste(GLYPH, mask=g)
    return img


def flat_small(glyph):
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    disc(ImageDraw.Draw(img), R_SMALL, None, (SMALL_LEFT, SMALL_RIGHT))
    g = square(place(glyph, SMALL_GLYPH_SCALE, SMALL_GLYPH_DROP), SMALL_BOLD, True)
    # Only within the circle, so the corners stay transparent.
    inside = Image.new("L", (W, W), 0)
    disc(ImageDraw.Draw(inside), R_SMALL, 255)
    img.paste(SMALL_GOLD, mask=ImageChops.multiply(g, inside))
    return img


def png_bytes(img):
    """The smallest lossless PNG: RGBA, or an exact palette when it fits."""
    candidates = []
    out = io.BytesIO()
    img.save(out, "PNG", optimize=True)
    candidates.append(out.getvalue())
    colors = img.getcolors(256)
    if colors:
        palette = [c for _, c in colors]
        index = {c: i for i, c in enumerate(palette)}
        p = Image.new("P", img.size)
        p.putdata([index[c] for c in img.get_flattened_data()])
        p.putpalette([v for c in palette for v in c[:3]])
        p.info["transparency"] = bytes(c[3] for c in palette)
        out = io.BytesIO()
        p.save(out, "PNG", optimize=True, transparency=p.info["transparency"])
        back = Image.open(io.BytesIO(out.getvalue())).convert("RGBA")
        if ImageChops.difference(back, img).getbbox() is None:
            candidates.append(out.getvalue())
    return min(candidates, key=len)


def main():
    args = sys.argv[1:]
    site = "--site" in args
    args = [a for a in args if a != "--site"]
    if len(args) != 2:
        sys.exit(__doc__)
    source, out_dir = Path(args[0]), Path(args[1])
    out_dir.mkdir(parents=True, exist_ok=True)

    src = Image.open(source).convert("RGB")
    if src.size != (W, W):
        sys.exit(f"expected a {W}x{W} source, got {src.size}")
    glyph = segment(src)
    big, big_small = flat(glyph), flat_small(glyph)

    images = {}
    for size in SIZES:
        images[f"flat_{size}"] = big.resize((size, size), Image.Resampling.LANCZOS)
    for size in SMALL_SIZES:
        images[f"flat-small_{size}"] = big_small.resize((size, size), Image.Resampling.LANCZOS)
    for name, img in images.items():
        (out_dir / f"{PREFIX}{name}.png").write_bytes(png_bytes(img))

    ico = [images["flat-small_16"], images["flat-small_32"], images["flat_48"], images["flat_64"]]
    # Pillow leaves out any size larger than the image it saves from, so
    # that's the 64; it stores each provided size as given.
    ico[-1].save(out_dir / "favicon.ico", sizes=[i.size for i in ico], append_images=ico[:-1])
    with Image.open(out_dir / "favicon.ico") as check:
        for img in ico:
            frame = check.ico.getimage(img.size).convert("RGBA")
            if frame.size != img.size:
                sys.exit(f"favicon.ico has no {img.size} frame")
            if ImageChops.difference(frame, img).getbbox() is not None:
                sys.exit(f"favicon.ico's {img.size} frame isn't the {img.size} icon")

    if site:
        for name, site_name in [
            ("flat-small_16", "chirho-code-flat-small-16.png"),
            ("flat-small_32", "chirho-code-flat-small-32.png"),
            ("flat_48", "chirho-code-flat-48.png"),
            ("flat_64", "chirho-code-flat-64.png"),
            ("flat_180", "chirho-code-flat-180.png"),
        ]:
            shutil.copyfile(out_dir / f"{PREFIX}{name}.png", SITE_BRAND / site_name)
        shutil.copyfile(out_dir / "favicon.ico", SITE_BRAND / "favicon.ico")

    for path in sorted(out_dir.iterdir()):
        print(f"{path.stat().st_size:>8}  {path}")


if __name__ == "__main__":
    main()
