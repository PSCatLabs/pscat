"""Paper grain and mottle of a scan or a render (issues #466, #474).

    python3 -I docs/dev/design/typewriter/tools/grain.py IMG [x0 y0 x1 y1]

Greyscale 0..1. The image is cut into 48px windows; a window counts as bare
paper when its darkest pixel is above 0.6 (no ink, no crease). For those:
  mean     mean brightness
  grain    root-mean-square over their 6px blocks of the block's standard deviation
           (the fine grain; 0.009 in T14, T2 and T9)
  mottle   standard deviation of the 48px windows' means (the slow variation)
Measure at the scan's own resolution (a render at 300 dpi against a 300 dpi
scan). Needs Pillow and numpy.
"""
import sys

import numpy as np
from PIL import Image


def measure(a, win=48, blk=6, floor=0.6):
    h, w = a.shape
    means, grains = [], []
    for y in range(0, h - win + 1, win):
        for x in range(0, w - win + 1, win):
            t = a[y:y + win, x:x + win]
            if t.min() < floor:
                continue
            means.append(t.mean())
            b = t.reshape(win // blk, blk, win // blk, blk).swapaxes(1, 2).reshape(-1, blk * blk)
            grains.append(float(np.sqrt(b.var(axis=1).mean())))
    if not means:
        return None
    return len(means), float(np.mean(means)), float(np.mean(grains)), float(np.std(means))


def main():
    im = Image.open(sys.argv[1]).convert("L")
    if len(sys.argv) > 5:
        im = im.crop(tuple(int(v) for v in sys.argv[2:6]))
    r = measure(np.asarray(im).astype(float) / 255)
    if r is None:
        print("no bare-paper windows")
        return
    n, mean, grain, mottle = r
    print(f"windows {n}  mean {mean:.3f}  grain {grain:.4f}  mottle {mottle:.4f}")


if __name__ == "__main__":
    main()
