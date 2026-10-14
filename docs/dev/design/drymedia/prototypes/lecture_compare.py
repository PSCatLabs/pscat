"""Builds docs/dev/design/drymedia/lecture_compare_238.png (#238).

The gallery still of gallery/chalkboard_lecture.ps across the top, three
real lecture boards from the #233 reference board under it, and 1:1
details of the render beside real crops at a similar scale. From the
repo root:

    python3 -I docs/dev/design/drymedia/prototypes/lecture_compare.py docs/dev/design/drymedia/refs \
        gallery/renders/chalkboard_lecture.png docs/dev/design/drymedia/lecture_compare_238.png

Needs Pillow. The detail boxes are pixel boxes in the 2x still
(2880x1700).
"""
import sys
from PIL import Image, ImageDraw, ImageFont

refs, still, out = sys.argv[1:4]


def load_font(size):
    try:
        return ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', size)
    except OSError:
        return ImageFont.load_default()


font = load_font(22)
small = load_font(17)
ink = (40, 40, 38)
W = 1800
pad, lab = 20, 32
render = Image.open(still).convert('RGB')
top = render.resize((W, W * render.height // render.width), Image.LANCZOS)


def fit_h(img, h):
    return img.resize((img.width * h // img.height, h), Image.LANCZOS)


def ref(name):
    return Image.open(f'{refs}/{name}.jpg').convert('RGB')


row2_h = 440
row2 = [
    ('C7 chalk_full_board (Math01, MathKnight, CC BY 2.5)', fit_h(ref('chalk_full_board'), row2_h)),
    ('C1 chalk_crypto_board (David Malone, CC BY 4.0)', fit_h(ref('chalk_crypto_board'), row2_h)),
    ('C11 chalk_fermi_circle (Fermi, Smithsonian, public domain)', fit_h(ref('chalk_fermi_circle'), row2_h)),
]
det_h = 360
row3 = [
    ('C5 ghosting close-up (Shixart1985, CC BY 2.0)', fit_h(ref('chalk_ghosting_closeup').crop((0, 200, 670, 560)), det_h)),
    ('render 1:1: last lecture ghosting under the fresh series', render.crop((60, 420, 960, 780)).resize((900 * det_h // 360, det_h))),
    ('C1 letters (CC BY 4.0)', fit_h(ref('chalk_crypto_letters'), det_h)),
]
row4 = [
    ('render 1:1: the boxed result, yellow chalk pressed hard', render.crop((1000, 1180, 1760, 1440))),
    ('render 1:2: the freehand unit circle', render.crop((1960, 280, 2700, 1020)).resize((360, 360), Image.LANCZOS)),
    ('C3 Einstein board fraction bars (decltype, CC BY-SA 3.0)', fit_h(ref('chalk_einstein_fraction_bars'), 260)),
]


def row_height(row):
    return max(im.height for _, im in row) + lab


H = (40 + top.height + lab + pad + sum(row_height(r) + pad for r in (row2, row3, row4)) + 60)
canvas = Image.new('RGB', (W + 2 * pad, H), (248, 247, 243))
d = ImageDraw.Draw(canvas)
y = pad
d.text((pad, y), 'gallery/chalkboard_lecture.ps (pscat, the 2x gallery still) against real lecture boards from docs/dev/design/drymedia/refs', font=font, fill=ink)
y += 34
canvas.paste(top, (pad, y))
y += top.height + pad
for row in (row2, row3, row4):
    x = pad
    for caption, im in row:
        if x + im.width > W + pad:
            im = im.crop((0, 0, W + pad - x, im.height))
        d.text((x, y), caption, font=small, fill=ink)
        canvas.paste(im, (x, y + lab - 8))
        x += im.width + pad
    y += row_height(row) + pad
d.text((pad, y), 'Licence: the panel is CC BY-SA 4.0 (it contains the CC BY-SA 3.0 C3 crop; as chalk_compare_235.jpg), with every attribution above; the render is pscat output.', font=small, fill=ink)
canvas.save(out, optimize=True)
