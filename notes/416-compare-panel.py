# Builds docs/two_compare_416.png: every committed render that changed with the
# handscript 2 (#416), before (left) and after (right) for each changed spot.
#
#   python3 -I notes/416-compare-panel.py BEFORE_DIR AFTER_DIR OUT
#
# BEFORE_DIR / AFTER_DIR hold same-named PNGs rendered from the base tree and
# this branch (the commands are in notes/416-handscript-two-base.md). The spots
# are found, not hand-picked: differing pixels are binned in 24px blocks and
# neighbouring blocks merged, so each tile is one cluster of changed ink (a 2,
# or in hs-write output the 2 plus the glyphs after it, whose jitter reshuffles).
import os
import sys
from PIL import Image, ImageChops, ImageDraw, ImageFont

B, A, OUT = sys.argv[1:4]
NAMES = [
    ('gallery/renders/chalkboard_lecture.png', 'chalkboard_lecture.png'),
    ('gallery/renders/field_notes.png', 'field_notes.png'),
    ('docs/drymedia/strokeglyphs_specimen.png', 'strokeglyphs_specimen.png'),
    ('docs/drymedia/strokeglyphs_subscripts_greek_specimen.png', 'strokeglyphs_subscripts_greek_specimen.png'),
    ('docs/drymedia/strokeglyphs_board_math_specimen.png', 'strokeglyphs_board_math_specimen.png'),
    ('docs/drymedia/mathkit_specimen.png', 'mathkit_specimen.png'),
]
try:
    font = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 26)
    small = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 18)
except OSError:
    font = small = ImageFont.load_default()

BS = 24


def clusters(a, b):
    d = ImageChops.difference(a, b).convert('L').point(lambda v: 255 if v > 8 else 0)
    w, h = d.size
    sm = d.resize((w // BS + 1, h // BS + 1), Image.BOX).point(lambda v: 255 if v > 0 else 0)
    px = sm.load()
    w2, h2 = sm.size
    seen, boxes = set(), []
    for y in range(h2):
        for x in range(w2):
            if px[x, y] and (x, y) not in seen:
                st = [(x, y)]
                seen.add((x, y))
                xs, ys = [x], [y]
                while st:
                    cx, cy = st.pop()
                    for dx in (-2, -1, 0, 1, 2):
                        for dy in (-2, -1, 0, 1, 2):
                            n = (cx + dx, cy + dy)
                            if 0 <= n[0] < w2 and 0 <= n[1] < h2 and px[n] and n not in seen:
                                seen.add(n)
                                st.append(n)
                                xs.append(n[0])
                                ys.append(n[1])
                boxes.append((min(xs) * BS, min(ys) * BS, (max(xs) + 1) * BS, (max(ys) + 1) * BS))
    return sorted(boxes, key=lambda t: (t[1] // 200, t[0]))


PAD = 10
ROWW = 1800
# extra zoomed tiles (file, box, zoom) for spots too small to read at 1x
ZOOMS = {'field_notes.png': [((230, 1024, 278, 1054), 6)]}
sections = []
for label, f in NAMES:
    a = Image.open(f'{B}/{f}').convert('RGB')
    b = Image.open(f'{A}/{f}').convert('RGB')
    pairs = []
    for x0, y0, x1, y1 in clusters(a, b):
        box = (max(0, x0 - PAD), max(0, y0 - PAD), min(a.width, x1 + PAD), min(a.height, y1 + PAD))
        ca, cb = a.crop(box), b.crop(box)
        z = max(1, min(4, int(250 / ca.width), int(160 / ca.height)))
        if z != 1:
            ca = ca.resize((ca.width * z, ca.height * z), Image.LANCZOS)
            cb = cb.resize((cb.width * z, cb.height * z), Image.LANCZOS)
        pairs.append((ca, cb))
    for box, z in ZOOMS.get(f, []):
        ca = a.crop(box).resize(((box[2] - box[0]) * z, (box[3] - box[1]) * z), Image.LANCZOS)
        cb = b.crop(box).resize(((box[2] - box[0]) * z, (box[3] - box[1]) * z), Image.LANCZOS)
        pairs.append((ca, cb))
    sections.append((label, pairs))

# flow layout: each pair is before|after with a thin rule between
GAP = 14
rows_by_section = []
for label, pairs in sections:
    rows, row, x = [], [], 0
    for ca, cb in pairs:
        pw = ca.width + cb.width + 8
        if row and x + pw > ROWW:
            rows.append(row)
            row, x = [], 0
        row.append((ca, cb))
        x += pw + GAP
    if row:
        rows.append(row)
    rows_by_section.append((label, len(pairs), rows))

H = 70
for label, n, rows in rows_by_section:
    H += 46 + sum(max(ca.height for ca, _ in r) + GAP for r in rows) + 10
out = Image.new('RGB', (ROWW + 40, H), 'white')
d = ImageDraw.Draw(out)
d.text((20, 15), 'issue #416: handscript 2, before (left of each pair) / after (right)', fill='black', font=font)
y = 60
for label, n, rows in rows_by_section:
    d.text((20, y), f'{label}  ({n} changed spots)', fill=(60, 60, 60), font=small)
    y += 36
    for r in rows:
        x = 20
        h = max(ca.height for ca, _ in r)
        for ca, cb in r:
            out.paste(ca, (x, y))
            d.rectangle((x - 2, y - 2, x + ca.width + cb.width + 9, y + ca.height + 1), outline=(200, 200, 200))
            d.line((x + ca.width + 4, y, x + ca.width + 4, y + h), fill=(200, 60, 60))
            out.paste(cb, (x + ca.width + 8, y))
            x += ca.width + cb.width + 8 + GAP
        y += h + GAP
    y += 10
out.crop((0, 0, out.width, y)).save(OUT, optimize=True)
