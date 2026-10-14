"""Builds docs/drymedia/crayon_compare.png and crayon_compare_figure.png (#239).

Reference crops on the left, pscat renders on the right, at about the
crops' scale (259 dpi, about 3.6 px per point). From the repo root:

    pscat --page 840x1000 --dpi 259 --png /tmp/spec259.png examples/drycrayon_specimen.ps
    pscat --page 840x760 --dpi 259 --png /tmp/motion259.png examples/drycrayon_motion_demo.ps
    python3 -I docs/drymedia/prototypes/crayon_compare.py docs/drymedia/refs \
        /tmp/spec259.png /tmp/motion259.png docs/drymedia/crayon_compare.png \
        docs/drymedia/crayon_compare_figure.png "<licence caption for the figure panel>"

Needs Pillow. The crop boxes follow the specimens' layout.
"""
import sys
from PIL import Image, ImageDraw, ImageFont
refs, spec, motion, out1, out2, tad = sys.argv[1:7]
s = 259/72.0
def ours(img, x, y, w, h, H):
    # PS-point box -> pixel crop
    return img.crop((int(x*s), int((H-y-h)*s), int((x+w)*s), int((H-y)*s)))
try:
    font = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 22)
except Exception:
    font = ImageFont.load_default()
sp = Image.open(spec).convert('RGB')
fg = Image.open(f'{refs}/crayon_fill_ground.jpg').convert('RGB')
fs = Image.open(f'{refs}/crayon_fill_streaks.jpg').convert('RGB')
W, Hc = 380, 200
rows = [
  ('Y2 light fill (pink): reference', fg.crop((300,215,680,415)),
   'dycrayon /Touch 0.6, dyscribble', ours(sp, 32, 58, W/s, Hc/s, 1000)),
  ('Y1 denser fill (grey): reference', fs.crop((150,330,530,530)),
   'dycrayon /Touch 0.72', ours(sp, 167, 58, W/s, Hc/s, 1000)),
  ('Y2 heavy press (black feet): reference', fg.crop((400,40,780,240)),
   'dycrayon /Touch 0.88', ours(sp, 302, 58, W/s, Hc/s, 1000)),
]
pad, lab = 20, 34
img = Image.new('RGB', (pad*3 + W*2, pad + len(rows)*(Hc+lab+pad) + 40), (255,255,255))
d = ImageDraw.Draw(img)
y = pad
for la, a, lb, b in rows:
    d.text((pad, y), la, fill=(60,60,60), font=font)
    d.text((pad*2+W, y), lb, fill=(60,60,60), font=font)
    img.paste(a.resize((W,Hc)), (pad, y+lab)); img.paste(b.resize((W,Hc)), (pad*2+W, y+lab))
    y += Hc + lab + pad
d.text((pad, y), 'Left: CC0 crops (MrAlanKoh, Wikimedia Commons). Right: pscat, 259 dpi (about the crops\' scale).', fill=(90,90,90), font=ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 16))
img.save(out1)
# figure: tadpole reference vs dyfigure tadpole
mo = Image.open(motion).convert('RGB')
t = Image.open(f'{refs}/crayon_child_tadpole_3y.jpg').convert('RGB')
tw, th = 440, int(440*t.height/t.width)
ofig = ours(mo, 428, 345, 192, 166, 760)
oh = int(ofig.height*tw/ofig.width)
img2 = Image.new('RGB', (pad*3 + tw*2, pad + lab + max(th, oh) + 50), (255,255,255))
d = ImageDraw.Draw(img2)
d.text((pad, pad), 'Y4 a 3-year-old\'s tadpole figure: reference', fill=(60,60,60), font=font)
d.text((pad*2+tw, pad), 'dyfigure /Tadpole true /Hair 9 + dycrayon', fill=(60,60,60), font=font)
img2.paste(t.resize((tw,th)), (pad, pad+lab)); img2.paste(ofig.resize((tw,oh)), (pad*2+tw, pad+lab))
d.text((pad, pad+lab+max(th,oh)+12), tad, fill=(90,90,90), font=ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 16))
img2.save(out2)
