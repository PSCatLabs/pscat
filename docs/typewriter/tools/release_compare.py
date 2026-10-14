"""Compare the released-document look with the reference crops (issue #214).

    python3 -I docs/typewriter/tools/release_compare.py [PSCAT] [OUT.png]

Run from the repo root. It renders examples/released_specimen.ps at 300 dpi
with pscat, cuts a crop of each sheet to the size of a reference crop in
points, scales it to the reference's own resolution, and stacks the scan above
it. The default output is docs/typewriter/release_vs_real.png.

Panels (REFERENCES.md observation numbers):
  T14  stamps and a redaction bar, on a copy of a typed page (300 dpi)
  T15  a bitonal release copy: filled counters, dropped hairlines, specks
       (300 dpi)
  T9   aged paper under typing (Szilard 1945, about 240 dpi)

Needs Pillow and a release pscat (default target/release/pscat). The scans are
real released documents and are references only: the specimen's own stamps,
names and text are invented.
"""
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
TW = os.path.dirname(HERE)
ROOT = os.path.abspath(os.path.join(TW, "..", ".."))
REFS = os.path.join(TW, "refs")

DPI = 300
PAGE_H = 520
SHEET_X = {0: 20, 1: 628, 3: 1236}   # the specimen's sheet origins, points
SHEET_TOP = PAGE_H - 36 - 480         # a sheet's top edge, points from the page top

# (reference, caption, reference dpi, [(generation, x, y, w, h) crops in sheet points from its top left])
PANELS = [
    ("T14_nsam55_marks.jpg", "T14 NSAM 55 (1961): a date stamp, a redaction bar, a copy. Below: generation 1",
     300, [(1, 345, 14, 252, 60), (1, 50, 170, 252, 60)]),
    ("T15_jfk_release_copy.jpg", "T15 a bitonal release copy. Below: generation 3",
     300, [(3, 54, 112, 261, 77)]),
    ("T10_szilard_zoom.jpg", "T10 Szilard (1945), aged paper, about 240 dpi. Below: generation 0",
     240, [(0, 54, 110, 338, 103)]),
]


def render(pscat, tmp):
    out = os.path.join(tmp, "spec.png")
    subprocess.run([pscat, "--page", "1876x520", "--dpi", str(DPI), "--png", out,
                    os.path.join(ROOT, "examples", "released_specimen.ps")],
                   check=True, cwd=ROOT, stdout=subprocess.DEVNULL)
    return Image.open(out).convert("RGB")


def crop(spec, gen, x, y, w, h, ref_dpi):
    s = DPI / 72.0
    px, py = (SHEET_X[gen] + x) * s, (SHEET_TOP + y) * s
    im = spec.crop((round(px), round(py), round(px + w * s), round(py + h * s)))
    k = ref_dpi / DPI
    return im.resize((round(im.width * k), round(im.height * k)), Image.LANCZOS)


def main():
    pscat = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "pscat")
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(TW, "release_vs_real.png")
    font = ImageFont.load_default()
    with tempfile.TemporaryDirectory() as tmp:
        spec = render(pscat, tmp)
    rows = []
    for ref, caption, ref_dpi, crops in PANELS:
        ims = [Image.open(os.path.join(REFS, ref)).convert("RGB")]
        ims += [crop(spec, *c, ref_dpi) for c in crops]
        rows.append((caption, ims))
    width = max(im.width for _, ims in rows for im in ims) + 20
    height = sum(14 + sum(im.height + 6 for im in ims) + 10 for _, ims in rows)
    board = Image.new("RGB", (width, height), (255, 255, 255))
    d = ImageDraw.Draw(board)
    y = 0
    for caption, ims in rows:
        d.text((10, y + 2), caption, fill=(60, 60, 60), font=font)
        y += 14
        for im in ims:
            board.paste(im, (10, y))
            d.rectangle((9, y - 1, 10 + im.width, y + im.height), outline=(190, 190, 190))
            y += im.height + 6
        y += 10
    board.save(out)
    print("wrote", out)


if __name__ == "__main__":
    main()
