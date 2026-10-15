"""Build docs/dev/design/drymedia/drycharcoal_refs_compare.png for #241 (rebuilt in #418):
five real charcoal references, each beside a crop of examples/drycharcoal_specimen.ps.

    cargo build --release --bin pscat
    python3 docs/dev/design/drymedia/prototypes/drycharcoal_refs_compare.py   # from the repo root

The specimen is rendered once, at 576 dpi (8 px per point), and the right-hand
crops are cut from that render in the specimen's own point coordinates (the
CROP_* boxes below are the `.ps` source's, before its `0 -70 translate`), so
the panel always shows the current specimen, whatever the paper. A cell the
size of one hatch square (28 pt, 224 px) is pasted at its native 8 px/pt; the
larger crops are scaled down (LANCZOS), the factor stated in each
caption as px/pt. Left-hand crops are the references in
docs/dev/design/drymedia/refs/ (see REFERENCES.md), scaled to a 300 px row
(H6, a tall sheet of three contour studies, is cut to its middle one).

Rows: H2 light hatching (pencil hatch at values 0.2, 0.4, 0.6), H4 fine
hatching (the sphere hatched to a value procedure), H1 stumped tone (vine
hatch smudged to 0.6), H6 stick strokes (vine and compressed dhstroke), H7
compressed mass (smudge, kneaded and hard lifts).
"""
import subprocess
import sys
import tempfile
from PIL import Image, ImageDraw, ImageFont

BIN = "target/release/pscat"
SPECIMEN = "examples/drycharcoal_specimen.ps"
REFS = "docs/dev/design/drymedia/refs/"
OUT = "docs/dev/design/drymedia/drycharcoal_refs_compare.png"

PAGE_W, PAGE_H = 720, 490      # the specimen's %%BoundingBox
SHIFT = 70                     # its `0 -70 translate`
DPI = 576
PX = DPI // 72                 # 8 px per point
ROW_H = 300                    # height of a reference crop
PAD, CAP = 16, 26


def spec_box(x0, y0, x1, y1):
    """A box in the specimen's source coordinates (y up, before the -70 shift)
    as a PIL box on the render (y down)."""
    return (x0 * PX, (PAGE_H - (y1 - SHIFT)) * PX,
            x1 * PX, (PAGE_H - (y0 - SHIFT)) * PX)


def cells(col0, i0, i1):
    """The 28 pt value-scale cells i0..i1 of the scale whose column starts at col0."""
    return [spec_box(col0 + 54 * i, 348, col0 + 54 * i + 28, 376) for i in range(i0, i1 + 1)]


# (reference file, reference crop box or None, caption, specimen boxes, px per point of the
# specimen crops)
ROWS = [
    ("charcoal_hatching_on_tone.jpg", None,
     "left: reference H2 light hatching on tooth",
     "right: drycharcoal, dhhatch pencil at values 0.2, 0.4 and 0.6",
     cells(24, 1, 3), 8),
    ("charcoal_fine_hatching.jpg", None,
     "left: reference H4 fine hatching over tone",
     "right: drycharcoal, dhhatch to a value procedure",
     [spec_box(58, 124, 158, 224)], 3),
    ("charcoal_stumped_tone.jpg", None,
     "left: reference H1 stumped tone",
     "right: drycharcoal, vine hatch + dhsmudge at 0.6",
     cells(380, 3, 3), 8),
    ("charcoal_stick_strokes.jpg", (20, 330, 640, 630),
     "left: reference H6 stick strokes",
     "right: drycharcoal, dhstroke vine and compressed",
     [spec_box(20, 420, 460, 474)], 2.5),
    ("charcoal_compressed_mass.jpg", None,
     "left: reference H7 compressed mass, lifts",
     "right: drycharcoal, compressed, dhsmudge, dhlift",
     [spec_box(280, 126, 416, 226)], 3),
]


def font(size):
    for name in ("/System/Library/Fonts/Helvetica.ttc",
                 "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


def render_specimen():
    with tempfile.TemporaryDirectory() as d:
        png = f"{d}/specimen.png"
        subprocess.run([BIN, "--page", f"{PAGE_W}x{PAGE_H}", "--dpi", str(DPI), "--png", png, SPECIMEN],
                       check=True, stdout=subprocess.DEVNULL)
        return Image.open(png).convert("RGB")


def scaled(im, factor):
    if factor == 1:
        return im
    return im.resize((round(im.width * factor), round(im.height * factor)), Image.LANCZOS)


def main():
    spec = render_specimen()
    if spec.size != (PAGE_W * PX, PAGE_H * PX):
        sys.exit(f"unexpected render size {spec.size}")
    panels = []
    for ref_file, ref_box, cap1, cap2, boxes, ppp in ROWS:
        ref = Image.open(REFS + ref_file).convert("RGB")
        if ref_box:
            ref = ref.crop(ref_box)
        else:
            ref = ref.resize((round(ref.width * ROW_H / ref.height), ROW_H), Image.LANCZOS)
        crops = [scaled(spec.crop(b), ppp / PX) for b in boxes]
        panels.append((ref, crops, cap1, f"{cap2} ({ppp:g} px/pt)"))

    width = max(ref.width + sum(c.width + PAD for c in crops) for ref, crops, _, _ in panels) + 2 * PAD
    row_hs = [max([ref.height] + [c.height for c in crops]) for ref, crops, _, _ in panels]
    height = PAD + sum(2 * CAP + h + PAD for h in row_hs)
    img = Image.new("RGB", (width, height), "white")
    dr = ImageDraw.Draw(img)
    f = font(15)
    y = PAD
    for (ref, crops, cap1, cap2), h in zip(panels, row_hs):
        dr.text((PAD, y), cap1, fill="black", font=f)
        dr.text((PAD, y + CAP // 2 + 2), cap2, fill="black", font=f)
        y += 2 * CAP - 6
        img.paste(ref, (PAD, y))
        x = PAD + ref.width + PAD
        for c in crops:
            img.paste(c, (x, y))
            x += c.width + PAD
        y += h + PAD + 6
    img.save(OUT, optimize=True)
    print(OUT, img.size)


main()
