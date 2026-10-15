# #520: the /worn preset's ribbon is 0.55

Follow-up to #511 (`notes/511-elite-ink-weight.md`), which left T10 (Szilard's worn
elite, `ink_compare.py`'s third panel) at about x0.67 of its scan. One constant in
`lib/typewriter.ps`: `/worn`'s `/Ribbon` 0.35 -> 0.55.

## Measured

The same measure as #511: each panel typed at `PANELS`' own options and
resolution, seeds 1-8 (9-16 in brackets), the scan scaled to the render's
resolution, paper normalised to 255 at its 95th percentile, **dark pixels (below
128) per non-space character** over a fixed number of cells from the first ink
column (28 pica, 13 elite for T4, 37 for T10), the rows of the main line plus 6 px.
The scratch script was rebuilt (not committed). Two details it needs that are easy
to get wrong: the T10 crop holds the lower half of the previous line at the top
(take the largest contiguous run of ink rows, not any row with ink), and it starts
with a cut-off "e" at the edge (skip a blob touching column 0, or the window sits two
cells off the render's). With both the baseline reproduces #511's numbers (T4
x1.01 for x1.02, T10 x0.66 for x0.67).

| panel | before | after |
|---|---|---|
| T10 worn elite, dark px per char | x0.66 (x0.68) | x0.76 (x0.78) |
| T10, soft mass | x0.65 (x0.66) | x0.72 (x0.73) |
| T4 elite (manual) | x1.01 (x1.01) | x1.01, byte-identical |
| T2 pica (manual) | x0.89 (x0.91) | x0.89, byte-identical |

## What was tried (T10, dark px per char, seeds 1-8)

| change | T10 |
|---|---|
| none | x0.66 |
| `/worn` `/Ribbon` 0.45 / 0.5 / 0.55 / 0.6 / 0.65 / 0.7 | x0.71 / 0.74 / 0.76 / 0.78 / 0.80 / 0.82 |
| elite bump off (`tEliteHole` 0) | x0.70 |
| `/Wear` 1.5 -> 1 | x0.69 |
| pressure sd 0.12 -> 0.09 / 0.06 / 0.03 | x0.67 / 0.70 / 0.72 |
| no little-finger factor | x0.72 |
| base hole level 0.15 -> 0.05 / 0 / -0.1 (every ribbon, so `/manual` too) | x0.72 / 0.75 / 0.79 |
| `/manual`'s parameters (`/Ribbon` 0.7, `/Wear` 1) on the same ink | x0.87 |
| flat ink (`/InkModel false`) | x1.21 (soft mass x1.03) |
| ink [0.29 0.26 0.23] instead of the harness's grey-brown, old preset | x0.79 |

`/Ribbon` is the one knob that is only the worn preset's (so `/manual` and
`/electric` cannot move) and that is a dry-ribbon quantity. The hole level at mean
pressure falls from 0.36 to 0.26 (0.21 at `/manual`) and the spread from 17 to 26
glyph units. It stays below `/manual`'s 0.7 so the preset is still drier. Wear and
pressure sd move the weight too little to be worth the other keys they touch.

## What is left, and why it is not the preset's

Even at `/Ribbon` 0.7 (a `/manual` ribbon) the panel is x0.82, and `/manual`
parameters on the same ink are x0.87, so most of the remaining gap is the ink: a
darker one alone is worth more (x0.79) than this change. The harness's ink renders at about 97/255 after normalising; the scan's
strokes reach 78-93 (0.5th to 3rd percentile of the window), so a lighter ink loses
dark pixels at the 128 threshold. The mean darkness of the dark pixels matches
(0.60 scan, 0.59 render), so the tail may be JPEG or resize overshoot. And on seed
10 the light-strike thinning (#477) drops the e of "the", the a of "phase" and the
final n nearly out, where the scan is even. Filed as #585. I did not change
`GREYBROWN` in `ink_compare.py`: it would improve the number without changing any
`/worn` page.

## Renders

`scripts/identity_check.sh <base> --loader lib/typewriter.ps --loader
lib/released.ps` (control: a `/worn` elite driver, differs as it must): 3 files,
only `examples/typewriter_specimen.ps` differs; `examples/released_specimen.ps` and
`gallery/released_memo.ps` are byte-identical. A manual and electric driver (elite
and pica) is byte-identical in both trees. The specimen's `/worn` line is the only
region of the specimen that changed (and it was not measured: its text is pica and
its ink is black, so the T10 weight does not apply to it).

Regenerated: `docs/dev/design/typewriter_specimen.png` (pscat, 200 dpi),
`typewriter_specimen_gs.png` (gs, the command in the specimen's header) and
`docs/dev/design/typewriter/ink_vs_real.png`; only the T10 rows changed in the last.
`notes/520-worn-elite-compare.png`: the scan, then pscat and gs before and after.
