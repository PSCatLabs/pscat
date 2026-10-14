"""Measure typewriter placement statistics on a scan.
usage: measure.py IMG DPI PITCH_PX [x0 y0 x1 y1]
Uses tesseract box output for character identity and boxes."""
import os, sys, subprocess, tempfile, collections
import numpy as np
from PIL import Image
img, dpi, pitch = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
im = Image.open(img).convert('L')
if len(sys.argv) > 7:
    x0, y0, x1, y1 = [int(v) for v in sys.argv[4:8]]
    im = im.crop((x0, y0, x1, y1))
path = os.path.join(tempfile.mkdtemp(), 'crop.png')
im.save(path)
H = im.height
A = np.asarray(im).astype(float)
out = subprocess.run(['tesseract', path, 'stdout', '--psm', '6', 'makebox'],
                     capture_output=True, text=True).stdout
boxes = []
for ln in out.splitlines():
    p = ln.split(' ')
    if len(p) < 6: continue
    ch = p[0]; a, b, c, d = map(int, p[1:5])
    top, bot = H - d, H - b
    if c - a < 2 or bot - top < 2: continue
    boxes.append((ch, a, top, c, bot))
pt = 72.0 / dpi
# group lines by vertical centre
boxes.sort(key=lambda t: (t[2] + t[4]) / 2)
lines = []; cur = []
for bx in boxes:
    cy = (bx[2] + bx[4]) / 2
    if cur and cy - np.median([(q[2] + q[4]) / 2 for q in cur]) > pitch * 0.9:
        lines.append(cur); cur = []
    cur.append(bx)
if cur: lines.append(cur)
BASE = set("ABCDEFGHIKLMNOPRSTUVWXYZabcdefhiklmnorstuvwxz0123456789")
dxs = collections.defaultdict(list); dys = collections.defaultdict(list)
ink = collections.defaultdict(list); cov = collections.defaultdict(list)
slopes = []; starts = []; intercepts = []
allx = []; ally = []
for L in lines:
    L.sort(key=lambda t: t[1])
    if len(L) < 10: continue
    xc = np.array([(t[1] + t[3]) / 2 for t in L])
    k = np.round((xc - xc[0]) / pitch)
    keep = np.ones(len(L), bool)
    for _ in range(3):
        P = np.polyfit(k[keep], xc[keep], 1)
        r = xc - np.polyval(P, k)
        keep = np.abs(r) < 0.35 * pitch
    bi = [i for i, t in enumerate(L) if t[0] in BASE and keep[i]]
    if len(bi) < 8: continue
    cov_line = []
    bx = np.array([xc[i] for i in bi]); by = np.array([L[i][4] for i in bi], float)
    keepb = np.ones(len(bi), bool)
    for _ in range(3):
        if keepb.sum() < 4: break
        Q = np.polyfit(bx[keepb], by[keepb], 1)
        rb = by - np.polyval(Q, bx)
        keepb = np.abs(rb) < 0.25 * pitch
    slopes.append(np.degrees(np.arctan(Q[0])))
    intercepts.append(np.polyval(Q, 0))
    starts.append((L[0][1], np.polyval(Q, 0)))
    for j, i in enumerate(bi):
        if not keepb[j]: continue
        ch = L[i][0]
        dys[ch].append(rb[j] * pt); ally.append(rb[j] * pt)
    for i, t in enumerate(L):
        if not keep[i]: continue
        dxs[t[0]].append(r[i] * pt); allx.append(r[i] * pt)
        if t[0].isalpha():
            reg = A[t[2]:t[4], t[1]:t[3]]
            m = reg < 128
            if m.sum() > 5:
                ink[t[0]].append((255 - reg[m].mean()) / 255)
                cov[t[0]].append(m.mean())
def summ(d, name):
    letters = {c: v for c, v in d.items() if len(v) >= 4}
    means = np.array([np.mean(v) for v in letters.values()])
    within = np.sqrt(np.mean([np.var(v) for v in letters.values()]))
    print(f"{name}: letters n>=4: {len(letters)}; spread of per-letter means (sd) {means.std():.3f} pt; "
          f"within-letter sd {within:.3f} pt")
    top = sorted(letters.items(), key=lambda kv: -abs(np.mean(kv[1])))[:5]
    print("   most offset letters:", ", ".join(f"{c}:{np.mean(v):+.2f}(n{len(v)})" for c, v in top))
print(f"{img}: lines used {len(slopes)}, chars {len(allx)}")
print(f"  all-char horizontal residual sd {np.std(allx):.3f} pt ; vertical (baseline) residual sd {np.std(ally):.3f} pt")
summ(dys, "  vertical")
summ(dxs, "  horizontal (confounded with glyph shape)")
print(f"  baseline slope per line: page skew {np.mean(slopes):+.3f} deg; per-line sd about it {np.std(slopes):.3f} deg, max dev {np.max(np.abs(np.array(slopes)-np.mean(slopes))):.3f} deg")
groups = collections.defaultdict(list)
for s, yb in starts: groups[round(s / pitch)].append((s, yb))
gs_ = []
for v in groups.values():
    if len(v) < 3: continue
    v = np.array(v); P2 = np.polyfit(v[:, 1], v[:, 0], 1)   # remove page skew (x0 drifting with y)
    gs_.append(np.std(v[:, 0] - np.polyval(P2, v[:, 1])) * pt)
print(f"  line-start wander (lines starting at same column): sd {np.mean(gs_) if gs_ else float('nan'):.3f} pt over {len(gs_)} groups")
ic = np.sort(np.array(intercepts)); d = np.diff(ic)
if len(d) > 2:
    base = np.median(d)
    print(f"  line advance: median {base*pt:.2f} pt, sd of advance {np.std(d[np.abs(d-base)<0.3*base])*pt:.3f} pt")
il = {c: v for c, v in ink.items() if len(v) >= 4}
mi = np.array([np.mean(v) for v in il.values()])
print(f"  ink darkness (0..1) of glyph pixels: overall mean {np.mean([x for v in ink.values() for x in v]):.2f}, "
      f"per-strike sd {np.sqrt(np.mean([np.var(v) for v in il.values()])):.3f}, per-letter mean sd {mi.std():.3f}")

cl = {c: v for c, v in cov.items() if len(v) >= 4}
cv = np.mean([np.std(v)/np.mean(v) for v in cl.values()])
print(f"  ink coverage of box per strike: coefficient of variation within letter {cv:.3f}")
ay=np.abs(np.array(ally)); ax=np.abs(np.array(allx))
print(f"  tails: |dy|>0.5pt {np.mean(ay>0.5)*100:.1f}%  |dy|>0.8pt {np.mean(ay>0.8)*100:.1f}%  |dx|>0.6pt {np.mean(ax>0.6)*100:.1f}%  |dx|>1pt {np.mean(ax>1.0)*100:.1f}%")

import json
if os.environ.get("LETTERS_JSON"):
    json.dump({"dy": dys, "dx": dxs}, open(os.environ["LETTERS_JSON"], "w"))
