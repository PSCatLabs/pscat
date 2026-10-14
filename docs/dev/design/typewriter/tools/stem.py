import os, sys, subprocess, tempfile, numpy as np
from PIL import Image
# usage: stem.py IMG DPI [crop x0 y0 x1 y1]: median dark-run width through middle rows of l/I/1/T stems; also x-height & cap height
img, dpi = sys.argv[1], float(sys.argv[2])
im = Image.open(img).convert('L')
if len(sys.argv) > 6: im = im.crop(tuple(int(v) for v in sys.argv[3:7]))
p = os.path.join(tempfile.mkdtemp(), 'crop.png'); im.save(p); A = np.asarray(im); H = im.height
out = subprocess.run(['tesseract', p, 'stdout', '--psm', '6', 'makebox'], capture_output=True, text=True).stdout
runs = []; xh = []; ch = []
for ln in out.splitlines():
    q = ln.split(' ')
    if len(q) < 6: continue
    c = q[0]; a, b, cc, d = map(int, q[1:5]); top, bot = H - d, H - b
    if c in 'xzvnmuo': xh.append(bot - top)
    if c in 'HEFTLIN': ch.append(bot - top)
    if c not in 'lI1': continue
    reg = A[top:bot, a:cc] < 140
    for r in reg[len(reg)//3: 2*len(reg)//3]:
        w = 0; best = 0
        for v in r:
            w = w + 1 if v else 0; best = max(best, w)
        if best: runs.append(best)
pt = 72 / dpi
print(f"{img}: stem width median {np.median(runs)*pt:.2f} pt (n={len(runs)} rows); x-height {np.median(xh)*pt:.2f} pt; cap height {np.median(ch)*pt:.2f} pt")
