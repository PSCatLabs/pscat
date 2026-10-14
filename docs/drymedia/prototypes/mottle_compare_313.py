"""Build docs/drymedia/mottle_compare_313.png for #313: drykit's /drawing paper
before and after its /Mottle went from 6 to 1.5.

Top row, the same fine pencil hatching and vine hatching drawn with
dhhatch on (1) the old default (/Mottle 6), (2) the old workaround
(/Relief 0.35) and (3) the new default, beside (4) a real charcoal
hatching crop (H2, docs/drymedia/refs/charcoal_hatching_on_tone.jpg).
Bottom row, Lincoln's face (gallery/lincoln_charcoal.ps) as committed on
main (/Relief 0.35, /Mottle 6) and now (the defaults).

    cargo build --release --bin pscat
    python3 docs/drymedia/prototypes/mottle_compare_313.py [BEFORE_REF]   # from the repo root

BEFORE_REF is a git ref holding the old gallery still (default 2d605be, the
commit before #313).
"""
import io
import subprocess
import sys
import tempfile
from PIL import Image, ImageDraw, ImageFont

BIN = "target/release/pscat"
REF = sys.argv[1] if len(sys.argv) > 1 else "2d605be"
OUT = "docs/drymedia/mottle_compare_313.png"

SWATCH = r"""%!PS
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/hatchkit.ps) run (lib/drycharcoal.ps) run
<< /Surface /drawing SURF /Seed 7 >> dksurface
userdict /dkGround get aload pop setrgbcolor 0 0 140 260 rectfill
0.11 0.10 0.10 setrgbcolor
newpath 5 135 moveto 135 135 lineto 135 255 lineto 5 255 lineto closepath
  << /Medium /vine /Value 0.55 /Angle 35 >> dhhatch
newpath 5 5 moveto 135 5 lineto 135 125 lineto 5 125 lineto closepath
  << /Medium /pencil /Value 0.75 /Angle 20 >> dhhatch
"""


def swatch(surf):
    with tempfile.TemporaryDirectory() as d:
        ps = f"{d}/s.ps"
        open(ps, "w").write(SWATCH.replace("SURF", surf))
        png = f"{d}/s.png"
        subprocess.run([BIN, "--page", "140x260", "--dpi", "144", "--png", png, ps],
                       check=True, stdout=subprocess.DEVNULL)
        return Image.open(png).convert("RGB")


def font(size):
    for name in ("/System/Library/Fonts/Helvetica.ttc",
                 "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


before = Image.open(io.BytesIO(subprocess.run(
    ["git", "show", f"{REF}:gallery/renders/lincoln_charcoal.png"],
    check=True, capture_output=True).stdout)).convert("RGB")
after = Image.open("gallery/renders/lincoln_charcoal.png").convert("RGB")
ref = Image.open("docs/drymedia/refs/charcoal_hatching_on_tone.jpg").convert("RGB")

FACE = (300, 300, 700, 700)
top = [
    ("old default (/Mottle 6)", swatch("/Mottle 6")),
    ("old workaround (/Relief 0.35)", swatch("/Relief 0.35 /Mottle 6")),
    ("new default (/Mottle 1.5)", swatch("")),
]
sw_w, sw_h = top[0][1].size
ref = ref.crop((0, 0, min(ref.width, 2 * sw_h // 3), ref.height))
ref = ref.resize((int(ref.width * sw_h / ref.height), sw_h))
top.append(("H2, real charcoal", ref))
bottom = [("Lincoln on main (Relief 0.35)", before.crop(FACE)),
          ("Lincoln now (defaults)", after.crop(FACE))]

PAD, CAP = 14, 26
width = sum(i.width for _, i in top) + PAD * (len(top) + 1)
row2_h = max(i.height for _, i in bottom)
height = PAD + CAP + sw_h + PAD + CAP + row2_h + PAD
img = Image.new("RGB", (width, height), "white")
dr = ImageDraw.Draw(img)
f = font(15)
x = PAD
for cap, im in top:
    dr.text((x, PAD), cap, fill="black", font=f)
    img.paste(im, (x, PAD + CAP))
    x += im.width + PAD
y = PAD + CAP + sw_h + PAD
x = PAD
for cap, im in bottom:
    dr.text((x, y), cap, fill="black", font=f)
    img.paste(im, (x, y + CAP))
    x += im.width + PAD
img.save(OUT, optimize=True)
print(OUT, img.size)
