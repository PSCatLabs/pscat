import os, sys, subprocess, tempfile, numpy as np
from PIL import Image
# monospace test: for each OCR line, fit x-centres of characters to an integer grid of the best pitch; report median residual / pitch.
img=sys.argv[1]; im=Image.open(img).convert('L')
if len(sys.argv)>5: im=im.crop(tuple(int(v) for v in sys.argv[2:6]))
p=os.path.join(tempfile.mkdtemp(), 'crop.png'); im.save(p); H=im.height
out=subprocess.run(['tesseract',p,'stdout','--psm','6','makebox'],capture_output=True,text=True).stdout
bx=[]
for ln in out.splitlines():
    q=ln.split(' ')
    if len(q)<6: continue
    a,b,c,d=map(int,q[1:5]); bx.append(((a+c)/2,H-(b+d)/2, q[0]))
bx.sort(key=lambda t:t[1]); lines=[]; cur=[]
for t in bx:
    if cur and t[1]-cur[-1][1]>15: lines.append(cur); cur=[]
    cur.append(t)
lines.append(cur)
xs=[np.array(sorted(x for x,_,_ in L)) for L in lines if len(L)>15]
best=None
for pitch in np.arange(12,45,0.25):
    res=[]
    for x in xs:
        ph=np.angle(np.exp(2j*np.pi*x/pitch).mean())
        r=((x/pitch - ph/(2*np.pi)+0.5)%1)-0.5
        res.append(np.median(np.abs(r)))
    sc=np.mean(res)
    if best is None or sc<best[0]: best=(sc,pitch)
print(f"{img}: best pitch {best[1]} px, median |residual| {best[0]:.3f} of pitch (monospace ~<0.08, random 0.25)")
