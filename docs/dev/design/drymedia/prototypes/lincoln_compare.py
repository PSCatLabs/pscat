"""Build docs/dev/design/drymedia/lincoln_compare.png for #242: the source photograph
beside the pscat render, over three real student charcoal studies from the
#233 reference board (all public domain; see docs/dev/design/drymedia/REFERENCES.md).

    python3 docs/dev/design/drymedia/prototypes/lincoln_compare.py   # from the repo root
"""
from PIL import Image, ImageDraw, ImageFont

PHOTO = "gallery/references/lincoln_berger_1864.jpg"
RENDER = "gallery/renders/lincoln_charcoal.png"
STUDIES = [
    ("docs/dev/design/drymedia/refs/charcoal_student_portrait_1888.jpg",
     "H3: Feilner, 3rd-year study, 1888"),
    ("docs/dev/design/drymedia/refs/charcoal_cast_study_1878.jpg",
     "Taklo (attr.), cast study, 1878"),
    ("docs/dev/design/drymedia/refs/charcoal_hatching_on_tone.jpg",
     "H2: hatching on tone (1888, detail)"),
]
OUT = "docs/dev/design/drymedia/lincoln_compare.png"

PAD, CAP = 16, 26
TOP_H, LOW_H = 600, 330


def font(size):
    for name in ("/System/Library/Fonts/Helvetica.ttc",
                 "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


def fit(path, h):
    im = Image.open(path).convert("RGB")
    return im.resize((round(im.width * h / im.height), h), Image.LANCZOS)


top = [(fit(PHOTO, TOP_H), "Source: Anthony Berger, Lincoln, 9 Feb 1864 (LoC, public domain)"),
       (fit(RENDER, TOP_H), "pscat: gallery/lincoln_charcoal.ps (drycharcoal on drykit)")]
low = [(fit(p, LOW_H), c) for p, c in STUDIES]

width = max(sum(i.width for i, _ in row) + PAD * (len(row) + 1) for row in (top, low))
height = PAD + CAP + TOP_H + PAD + CAP + LOW_H + PAD
page = Image.new("RGB", (width, height), "white")
draw = ImageDraw.Draw(page)
f = font(15)


def lay(row, y, h):
    x = (width - (sum(i.width for i, _ in row) + PAD * (len(row) - 1))) // 2
    for im, cap in row:
        draw.text((x, y), cap, fill=(30, 30, 30), font=f)
        page.paste(im, (x, y + CAP))
        x += im.width + PAD


lay(top, PAD, TOP_H)
lay(low, PAD + CAP + TOP_H + PAD, LOW_H)
page.save(OUT)
print(OUT, page.size)
