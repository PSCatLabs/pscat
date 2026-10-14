"""Builds docs/drymedia/crayon_drawing_compare.png (#240).

A 6-year-old's whole crayon picture (REFERENCES.md Y5, CC0) beside
gallery/crayon_drawing.ps, and three same-scale details under them: the sun's
yellow, a colour band across the top, and the ground. Only the CC0 rhino
drawing is used, so the panel is CC0 apart from pscat's own render. From the
repo root:

    python3 -I docs/drymedia/prototypes/crayon_drawing_compare.py \
        docs/drymedia/refs gallery/renders/crayon_drawing.png \
        docs/drymedia/crayon_drawing_compare.png

The still is the committed 2x render (1584x1224). Needs Pillow.
"""
import sys
from PIL import Image, ImageDraw, ImageFont

refs, still, out = sys.argv[1:4]


def load_font(size):
    try:
        return ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', size)
    except OSError:
        return ImageFont.load_default()


font, small = load_font(22), load_font(16)
ref = Image.open(f'{refs}/crayon_child_rhino_6y.jpg').convert('RGB')
ours = Image.open(still).convert('RGB')
W = 820
pad, lab = 20, 34


def fit(img, w):
    return img.resize((w, round(img.height * w / img.width)), Image.LANCZOS)


# Each detail is a pair: (label, crop of the rhino drawing, crop of our
# still), each resized to 400x200.
details = [
    ('yellow disc: Y5 | ours, yellow over blue', (70, 125, 470, 325), (30, 150, 430, 350)),
    ('band across the top: Y5 | ours, the sky', (470, 0, 870, 200), (700, 20, 1100, 220)),
    ('grey body fill: Y5 | ours, the house wall', (400, 260, 800, 460), (340, 640, 740, 840)),
    ('ground bands: Y5 | ours, grass and path', (250, 545, 650, 745), (330, 940, 730, 1140)),
]
a, b = fit(ref, W), fit(ours, W)
h1 = max(a.height, b.height)
rows = (len(details) + 1) // 2
H = pad + lab + h1 + pad + rows * (200 + lab + pad) + 40
img = Image.new('RGB', (pad * 3 + W * 2, H), (255, 255, 255))
d = ImageDraw.Draw(img)
d.text((pad, pad), 'Y5 a 6-year-old\'s picture: reference', fill=(60, 60, 60), font=font)
d.text((pad * 2 + W, pad), 'gallery/crayon_drawing.ps, pscat', fill=(60, 60, 60), font=font)
img.paste(a, (pad, pad + lab))
img.paste(b, (pad * 2 + W, pad + lab))
y0 = pad + lab + h1 + pad
for i, (label, ca, cb) in enumerate(details):
    x = pad + (i % 2) * (W + pad)
    y = y0 + (i // 2) * (200 + lab + pad)
    d.text((x, y), label, fill=(60, 60, 60), font=font)
    img.paste(ref.crop(ca).resize((400, 200)), (x, y + lab))
    img.paste(ours.crop(cb).resize((400, 200)), (x + 420, y + lab))
y = y0 + rows * (200 + lab + pad)
d.text((pad, y), 'Left: CC0 (MrAlanKoh, Wikimedia Commons). Right: pscat, 2x. This panel is CC0 apart from pscat\'s render.',
       fill=(90, 90, 90), font=small)
img.save(out)
