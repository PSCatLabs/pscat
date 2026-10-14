"""Blind comparison of the released-memo gallery piece with real scans (issue #215).

    python3 -I docs/typewriter/tools/blind_compare.py [PSCAT]

Run from the repo root. It renders gallery/released_memo.ps at 300 dpi with
pscat and writes two boards:

  docs/typewriter/blind_grid.png    six unlabelled strips, A to F, in a fixed
                                    shuffled order: four from real scans on
                                    the reference board and two from the
                                    render. Every strip is one typed line at
                                    one scale, 240 dpi (the Szilard scan's
                                    own), and one size, 720 x 64 px, all
                                    greyscale; the render's strips go through
                                    JPEG at quality 85 as the scans did. Which
                                    letters are the render is printed to
                                    stdout, not written to the repo: the key
                                    lives in the pull request.
  docs/typewriter/memo_vs_real.png  the whole page beside T1, the only full
                                    typed page on the board, at T1's scale
                                    (75 dpi: make_board.py's 0.25 of 300).

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

PAGE = (612, 792)
DPI = 300                 # the render's resolution
GRID_DPI = 240            # every strip's scale
TILE = (720, 64)          # every strip's size, pixels at GRID_DPI
T1_DPI = 75               # T1 is 0.25 of a 300 dpi scan

# Real strips: (file, the committed crop's dpi, (left, top) in pixels at
# GRID_DPI). A crop's dpi is its scan's times make_board.py's scale: T2 is
# 300 (resampled by 0.8 here), T9 0.6 of 240 = 144 (by 1.67, the one real
# enlargement of note), T10 1.5 of 240 = 360 (by 0.67), T15 0.7 of 300 = 210
# (by 1.14). T4 (600 dpi, 380 x 65 px at 240) is too small for a strip.
REAL = [
    ("T02_duncan_line.jpg", 300, (16, 2)),
    ("T09_szilard_block.jpg", 144, (20, 362)),
    ("T10_szilard_zoom.jpg", 360, (20, 8)),
    ("T15_jfk_release_copy.jpg", 210, (140, 222)),
]

# Render strips: (left, top) in points from the page's top left. Body text
# only, away from the header (invented names would be a tell either way) and
# the redaction bars.
RENDERED = [
    (72, 280),
    (150, 199),
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


def to_grid(im, dpi):
    k = GRID_DPI / dpi
    return im.resize((round(im.width * k), round(im.height * k)), Image.LANCZOS)


def real_strip(name, dpi, at):
    im = to_grid(grey(Image.open(os.path.join(REFS, name))), dpi)
    return im.crop((at[0], at[1], at[0] + TILE[0], at[1] + TILE[1]))


def render_strip(page, origin):
    s = DPI / 72.0
    w, h = (t * DPI / GRID_DPI for t in TILE)
    x, y = origin[0] * s, origin[1] * s
    im = page.crop((round(x), round(y), round(x + w), round(y + h)))
    return jpeg(grey(im.resize(TILE, Image.LANCZOS)))


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
    w = round(page.width * T1_DPI / DPI)
    h = round(page.height * T1_DPI / DPI)
    small = page.resize((w, h), Image.LANCZOS)
    font = ImageFont.load_default()
    pad, top = 16, 22
    b = Image.new("RGB", (t1.width + w + 3 * pad, max(h, t1.height) + top + pad), (255, 255, 255))
    d = ImageDraw.Draw(b)
    d.text((pad, 6), "Real: T1, a typed letter of 1914 (NARA 284501), the board's only full page", fill=(0, 0, 0), font=font)
    d.text((2 * pad + t1.width, 6), "Render: gallery/released_memo.ps (pscat), the same scale", fill=(0, 0, 0), font=font)
    b.paste(t1, (pad, top))
    b.paste(small, (2 * pad + t1.width, top))
    b.save(out)


def main():
    pscat = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "pscat")
    with tempfile.TemporaryDirectory() as tmp:
        page = render(pscat, tmp)
    tiles = [(name, real_strip(name, dpi, at)) for name, dpi, at in REAL]
    tiles += [("render at (%d, %d) pt" % o, render_strip(page, o)) for o in RENDERED]
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
