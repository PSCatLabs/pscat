"""Stem darkness against height above the baseline (issue #468, for #538).

A worn ribbon lane would show as stems that print grey in the middle of the
x-height and darker toward the tops of ascenders. This takes the vertical
stems of d, h, k, l and b (tesseract boxes), finds each line's baseline from
the bottoms of the letters with no ascender or descender, and averages the
stem's darkness at each height, two ways. "Peak" is 255 minus the darkest of
three adjacent columns: the stem's core, which a thinned stem can keep. "Mean"
is 255 minus the mean grey over a 7-column window centred on the stem: it falls
when the stem loses ink (holes, thinner strokes), which is what /WornBand does.

    python3 -I lane.py PLOT.png IMG:DPI[:x0:y0:x1:y1] [IMG:DPI ...]

Prints one table per scan (height in points, strikes, peak and mean darkness)
and saves a two-panel plot. Needs Pillow, NumPy, matplotlib and tesseract.
"""
import os
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image

BASE = set("acemnorsuvwxz")  # no ascender, no descender
STEM = set("dhklb")  # tall letters with a plain vertical stem


def profile(path, dpi, box=None):
    im = Image.open(path).convert("L")
    if box:
        im = im.crop(box)
    w, h = im.size
    tmp = os.path.join(tempfile.mkdtemp(), "c.png")
    im.save(tmp)
    a = np.asarray(im).astype(float)
    out = subprocess.run(["tesseract", tmp, "stdout", "--psm", "6", "makebox"],
                         capture_output=True, text=True).stdout
    boxes = []
    for ln in out.splitlines():
        p = ln.split(" ")
        if len(p) < 6:
            continue
        x0, y0, x1, y1 = map(int, p[1:5])
        boxes.append((p[0], x0, h - y1, x1, h - y0))
    boxes.sort(key=lambda t: (t[2] + t[4]) / 2)
    lines, cur = [], []
    for bx in boxes:
        cy = (bx[2] + bx[4]) / 2
        if cur and cy - np.median([(q[2] + q[4]) / 2 for q in cur]) > 0.047 * dpi:
            lines.append(cur)
            cur = []
        cur.append(bx)
    if cur:
        lines.append(cur)
    prof, n = {}, 0
    for line in lines:
        bots = [t[4] for t in line if t[0] in BASE]
        if len(bots) < 6:
            continue
        base = np.median(bots)
        for ch, x0, t0, x1, b0 in line:
            if ch not in STEM or b0 - t0 < 0.05 * dpi:
                continue
            reg = a[t0:b0, x0:x1]
            if reg.size == 0:
                continue
            rows = reg.shape[0]
            j = int(np.argmin(reg[int(rows * .3):int(rows * .7)].mean(axis=0)))
            col = reg[:, max(0, j - 1):j + 2].min(axis=1)
            win = reg[:, max(0, j - 3):j + 4].mean(axis=1)
            n += 1
            for r in range(rows):
                prof.setdefault(int(round(base - (t0 + r))), []).append(
                    (255 - col[r], 255 - win[r]))
    out = {}
    for k, v in prof.items():
        if len(v) >= 8:
            v = np.array(v)
            out[k] = (len(v), float(v[:, 0].mean()), float(v[:, 1].mean()))
    return n, out


def main():
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, axs = plt.subplots(1, 2, figsize=(8.0, 3.0), dpi=100, sharey=False)
    for spec in sys.argv[2:]:
        parts = spec.split(":")
        path, dpi = parts[0], float(parts[1])
        box = tuple(int(v) for v in parts[2:6]) if len(parts) >= 6 else None
        n, p = profile(path, dpi, box)
        ks = sorted(p)
        pt = [k * 72 / dpi for k in ks]
        label = os.path.basename(path).split(".")[0]
        print(f"{label}: {n} stems at {dpi:g} dpi")
        for k in ks:
            print(f"  {k * 72 / dpi:5.2f} pt  n={p[k][0]:4d}  peak {p[k][1]:6.1f}  mean {p[k][2]:6.1f}")
        for ax, i in zip(axs, (1, 2)):
            ax.plot(pt, [p[k][i] for k in ks], label=f"{label} ({n} stems)")
    for ax, t in zip(axs, ("peak (darkest of 3 columns)", "mean (7-column window)")):
        ax.set_title(t, fontsize=8)
        ax.set_xlabel("height above baseline (pt)")
        ax.set_ylabel("darkness (0-255)")
        ax.set_ylim(0, 255)
    axs[0].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(sys.argv[1])


if __name__ == "__main__":
    main()
