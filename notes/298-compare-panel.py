# Builds docs/specimen_polish_298_compare.png from before/ and after/ renders.
import sys
from PIL import Image, ImageDraw, ImageFont
B,A,OUT=sys.argv[1],sys.argv[2],sys.argv[3]
try: font=ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc',26)
except Exception: font=ImageFont.load_default()
rows=[
 ('canvas weave: /Touch sweep and /Colors load (paintkit_canvas_demo, 2x)','paintkit_canvas_demo.png',(40,720,1020,1200),1),
 ('knife snow pulls (paintkit_loading_demo, 2x)','paintkit_loading_demo.png',(1040,120,1520,600),1),
 ('evening_stream: touched marks on the new weave (2x still, lower half)','evening_stream.png',(300,628,1220,1120),1),
 ('handscript 7: field_notes axis (2x)','field_notes.png',(560,1015,860,1060),3),
 ('handscript 7 via strokeglyphs: chalkboard "Euler, 1748" (2x)','chalkboard_lecture.png',(1450,1470,1700,1550),3),
 ('handscript 7 via strokeglyphs: specimen digit row','strokeglyphs_specimen.png',(560,385,760,425),4),
]
pieces=[]
for label,f,box,z in rows:
    a=Image.open(f'{B}/{f}').convert('RGB').crop(box); b=Image.open(f'{A}/{f}').convert('RGB').crop(box)
    if z!=1: a=a.resize((a.width*z,a.height*z),Image.LANCZOS); b=b.resize((b.width*z,b.height*z),Image.LANCZOS)
    pieces.append((label,a,b))
W=max(p[1].width for p in pieces)*2+60; H=sum(p[1].height+60 for p in pieces)+60
out=Image.new('RGB',(W,H),'white'); d=ImageDraw.Draw(out)
d.text((20,15),'issue #298: before (left) / after (right)',fill='black',font=font)
y=60
for label,a,b in pieces:
    d.text((20,y),label,fill=(60,60,60),font=font); y+=40
    out.paste(a,(20,y)); out.paste(b,(40+a.width,y)); y+=a.height+20
out.save(OUT, optimize=True)
