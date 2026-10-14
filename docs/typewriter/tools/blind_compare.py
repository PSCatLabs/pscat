"""Blind comparison of the released-memo gallery piece with real scans (issue #215).

    python3 -I docs/typewriter/tools/blind_compare.py [PSCAT]

Run from the repo root. It renders gallery/released_memo.ps at 300 dpi with
pscat and writes two boards:

  docs/typewriter/blind_grid.png    six unlabelled crops, A to F, in a fixed
                                    shuffled order: four real scans from the
                                    reference board and two crops of the
                                    render. Each crop is at its scan's own
                                    resolution (a render crop takes the pixel
                                    size and scale of one of the scans), every crop
                                    is greyscale, and the render's crops go
                                    through JPEG at quality 85 as the scans
                                    did. Which letters are the render is
                                    printed to stdout, not written to the
                                    repo: the key lives in the pull request.
  docs/typewriter/memo_vs_real.png  the whole page beside T1, the only full
                                    typed page on the board, at the same
                                    physical scale (about 71 dpi).

Needs Pillow and a release pscat (default target/release/pscat). The scans
are real public-domain documents and are references only; the memo's text,
names, office and stamps are invented.
"""
import io
import os
import random
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
TW = os.path.dirname(HERE)
ROOT = os.path.abspath(os.path.join(TW, "..", ".."))
REFS = os.path.join(TW, "refs")
PIECE = os.path.join(ROOT, "gallery", "released_memo.ps")

DPI = 300
PAGE = (612, 792)

# Real scans: (file, dpi). Whole committed crops, as REFERENCES.md has them;
# the dpi is the crop's own, the scan's times make_board.py's scale (T4 was
# enlarged 2x from 300 dpi, T9 reduced to 0.6 of 240 dpi, T15 to 0.7 of 300).
REAL = [
    ("T02_duncan_line.jpg", 300),
    ("T04_fdr_filled_counters.jpg", 600),
    ("T09_szilard_block.jpg", 144),
    ("T15_jfk_release_copy.jpg", 210),
]

# Render crops: (left, top) in points from the page's top left, and the scan
# whose pixel size and dpi the crop takes. Body text only, away from the
# header (invented names would be a tell either way) and the redaction bars.
RENDERED = [
    ((96, 273), "T04_fdr_filled_counters.jpg", 600),
    ((26, 362), "T09_szilard_block.jpg", 144),
]

SHUFFLE_SEED = 215


def render(pscat, tmp):
    out = os.path.join(tmp, "memo.png")
    subprocess.run([pscat, "--page", "%dx%d" % PAGE, "--dpi", str(DPI), "--png", out, PIECE],
                   check=True, cwd=ROOT, stdout=subprocess.DEVNULL)
    return Image.open(out).convert("RGB")


def grey(im):
    return im.convert("L")


def jpeg(im, quality=85):
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=quality)
    buf.seek(0)
    return Image.open(buf).convert("L")


def render_crop(page, origin, ref, ref_dpi):
    w, h = Image.open(os.path.join(REFS, ref)).size
    s = DPI / 72.0
    k = DPI / ref_dpi                      # render pixels per scan pixel
    x, y = origin[0] * s, origin[1] * s
    im = page.crop((round(x), round(y), round(x + w * k), round(y + h * k)))
    im = im.resize((w, h), Image.LANCZOS)
    return jpeg(grey(im))


def board(tiles, out):
    font = ImageFont.load_default()
    width = max(im.width for _, im in tiles) + 40
    height = sum(im.height + 30 for _, im in tiles) + 10
    b = Image.new("L", (width, height), 255)
    d = ImageDraw.Draw(b)
    y = 10
    for label, im in tiles:
        d.text((10, y), label, fill=0, font=font)
        y += 16
        b.paste(im, (30, y))
        d.rectangle((29, y - 1, 30 + im.width, y + im.height), outline=160)
        y += im.height + 14
    b.save(out)


def full_page(page, out):
    t1 = Image.open(os.path.join(REFS, "T01_duncan_page_1914.jpg")).convert("RGB")
    w = t1.width                            # T1 is a whole letter sheet, 8.5 in wide
    h = round(page.height * w / page.width)
    small = page.resize((w, h), Image.LANCZOS)
    font = ImageFont.load_default()
    pad, top = 16, 22
    b = Image.new("RGB", (2 * w + 3 * pad, max(h, t1.height) + top + pad), (255, 255, 255))
    d = ImageDraw.Draw(b)
    d.text((pad, 6), "Real: T1, a typed letter of 1914 (NARA 284501), the board's only full page", fill=(0, 0, 0), font=font)
    d.text((2 * pad + w, 6), "Render: gallery/released_memo.ps (pscat), same scale", fill=(0, 0, 0), font=font)
    b.paste(t1, (pad, top))
    b.paste(small, (2 * pad + w, top))
    b.save(out)


def main():
    pscat = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "pscat")
    with tempfile.TemporaryDirectory() as tmp:
        page = render(pscat, tmp)
    tiles = [(name, grey(Image.open(os.path.join(REFS, name)))) for name, _ in REAL]
    tiles += [("render at (%d, %d) pt, as %s" % (o[0], o[1], ref), render_crop(page, o, ref, dpi))
              for o, ref, dpi in RENDERED]
    random.Random(SHUFFLE_SEED).shuffle(tiles)
    letters = "ABCDEFGH"
    board([(letters[i], im) for i, (_, im) in enumerate(tiles)],
          os.path.join(TW, "blind_grid.png"))
    full_page(page, os.path.join(TW, "memo_vs_real.png"))
    print("key (keep out of the repo):")
    for i, (name, _) in enumerate(tiles):
        print("  %s  %s" % (letters[i], name))


if __name__ == "__main__":
    main()
