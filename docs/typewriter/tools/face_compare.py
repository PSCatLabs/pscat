"""Base-face comparison for docs/TYPEWRITER.md (issue #210).

    python3 -I docs/typewriter/tools/face_compare.py CACHE_DIR FONT_DIR

CACHE_DIR is make_board.py's cache (duncan.png, szilard.png). FONT_DIR holds
CourierPrime-Regular.ttf and CutiveMono-Regular.ttf from github.com/google/fonts
(ofl/courierprime, ofl/cutivemono); Ghostscript's Nimbus Mono is read from
GS_FONT (default: Homebrew's n022003l.pfb). Each face is set at the scan's own
pitch, with the em scaled so the advance equals the pitch, in plain flat ink:
this compares letterforms, not ink. Writes docs/typewriter/base_face_compare.png.
"""
import os
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
TW = os.path.dirname(HERE)
ROOT = os.path.abspath(os.path.join(TW, "..", ".."))
cache, fdir = sys.argv[1], sys.argv[2]
GS_FONT = os.environ.get("GS_FONT", "/opt/homebrew/share/ghostscript/fonts/n022003l.pfb")
FACES = [
    ("Liberation Mono (pscat /Courier)", os.path.join(ROOT, "fonts", "LiberationMono-Regular.ttf")),
    ("Nimbus Mono (gs /Courier)", GS_FONT),
    ("Special Elite", os.path.join(ROOT, "fonts", "catalog", "SpecialElite", "SpecialElite-Regular.ttf")),
    ("Cutive Mono", os.path.join(fdir, "CutiveMono-Regular.ttf")),
    ("Courier Prime (chosen)", os.path.join(fdir, "CourierPrime-Regular.ttf")),
]
LAB = 360
label_font = ImageFont.truetype(os.path.join(ROOT, "fonts", "LiberationSans-Regular.ttf"), 20)
bold_font = ImageFont.truetype(os.path.join(ROOT, "fonts", "LiberationSans-Bold.ttf"), 20)


def labelled(im, text, bold=False):
    out = Image.new("L", (LAB + im.width, im.height), 255)
    ImageDraw.Draw(out).text((8, im.height // 2 - 12), text, fill=0 if bold else 80,
                             font=bold_font if bold else label_font)
    out.paste(im, (LAB, 0))
    return out


def setline(text, pitch, h, path):
    f = ImageFont.truetype(path, 100)
    size = int(round(100 * pitch / f.getlength("M")))
    f = ImageFont.truetype(path, size)
    im = Image.new("L", (int(pitch * (len(text) + 1)), h), 255)
    d = ImageDraw.Draw(im)
    for i, ch in enumerate(text):
        d.text((4 + i * pitch, h * 0.72), ch, fill=30, font=f, anchor="ls")
    return im


def block(real, caption, text, pitch, h):
    rows = [labelled(real, caption, True)]
    rows += [labelled(setline(text, pitch, h, p), name) for name, p in FACES]
    return rows


duncan = Image.open(os.path.join(cache, "duncan.png")).convert("L").crop((556, 1130, 1500, 1215))
szilard = Image.open(os.path.join(cache, "szilard.png")).convert("L").crop((250, 570, 1150, 650))
rows = block(duncan, "REAL 1914 pica (T2)", "In reply to your letter of June", 30, 85)
rows.append(Image.new("L", (10, 24), 255))
rows += block(szilard, "REAL 1945 elite (T10)", "e present phase of the war against Japan.", 20, 70)
W = max(r.width for r in rows)
sheet = Image.new("L", (W, sum(r.height + 4 for r in rows)), 255)
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height + 4
sheet.save(os.path.join(TW, "base_face_compare.png"))
print("wrote base_face_compare.png", sheet.size)
