# #511: elite and pica take the ribbon ink separately

Follow-up to #423 and #477 (`notes/423-ink-lighter-uneven.md`,
`notes/477-light-strike-thins.md`). Design in `docs/dev/design/TYPEWRITER.md`
("Tuned in #511"). Library change in `lib/typewriter.ps` (`tDerive`, `tInkStrike`,
two new constants) and one test.

## The problem

After #423/#477 elite was still about 6% heavy against T4 while pica (T2) was about
right. The light-strike erosion is the same on both faces, so it cannot separate them.

## What changed

- `tFaceK` = (`/Pitch` - 10) / 2 clamped to 0..1, set in `tDerive`: 0 for pica and
  coarser, 1 for elite and finer.
- The strike's hole level gets `tFaceK x tEliteHole x /Ribbon` added
  (`tEliteHole` 0.18; +0.126 at the default ribbon). A hole's area is a third of
  its level, so it takes about 4% of the elite's dark area. Not on a copy
  (`/Copies > 0`): the photocopy's contrast curve would scale the bump 2.3 to 3.5
  times and drop elite copies out. `/InkModel false` is untouched.
- Tried and dropped: a thinner body stroke for elite (the body stroke is 0.3 pixel at
  300 dpi, so 25% of it moved the weight 1%) and a base erosion (it makes every
  elite strike clip four times, where today only a light one does).
- `/Ribbon` scales the bump so `/worn` (ribbon 0.35, T10) gets half of it.

## Measured, before and after

Scratch script (not committed, `i511/meas.py`). Each panel of `ink_compare.py`'s
text, typed at `PANELS`' own options and resolution, seeds 1-8 (9-16 as a check, in brackets). The scan is scaled to the render's resolution, paper normalised to 255
at its 95th percentile. The window is a fixed number of cells from the first ink
column (28 pica cells for T2, 13 elite for T4, 37 for T10), the rows of the line
(plus 6 px); the measure is **dark pixels (below 128) per non-space character**,
and the "soft mass" is sum(max(0, darkness - 0.2)) per character. These are not
#423's and #477's bands (those quote T2 x0.98 and x0.93 by coverage), so only the
before/after change on this one measure is meant.

| render / scan | before | after |
|---|---|---|
| T4 elite, dark px per char | x1.06 (x1.06 on seeds 9-16) | x1.02 (x1.02) |
| T4 elite, soft mass | x1.04 (x1.04) | x0.99 (x0.99) |
| T2 pica, dark px per char | x0.87 | x0.87, byte-identical |
| T10 worn elite, dark px per char | x0.71 | x0.67 |

By this measure pica sits under its scan (x0.87 dark px, x0.84 soft mass); the
issue's x0.93 and #477's integrated-mass figures are different bands. Not changed
here, because the maintainer reads pica as about right. T10 is light before the
change (the grey-brown ribbon's strokes sit near the 128 threshold, so it is the
least reliable of the three), and the Szilard machine disagrees with the FDR one
about elite, so the per-face constant is a compromise.

## Renders redone

`scripts/identity_check.sh ../<base> --loader lib/typewriter.ps --loader
lib/released.ps` (control: an elite driver): 3 files, of which only
`examples/typewriter_specimen.ps` differs. `examples/released_specimen.ps` and
`gallery/released_memo.ps` are byte-identical (no elite page without copies). A
pica-only driver (manual, worn and `/Copies 2` lines) is byte-identical in both
trees, and `scripts/typewrite.sh`'s memo (pica) is byte-identical, so is
`gallery/renders/released_memo.png`.

Regenerated: `docs/dev/design/typewriter_specimen.png` (pscat, 200 dpi),
`typewriter_specimen_gs.png` (gs `-r200 -dGraphicsAlphaBits=4 -dTextAlphaBits=4`;
these reproduce the base file pixel for pixel), and
`docs/dev/design/typewriter/ink_vs_real.png` (`ink_compare.py`). Re-run
`release_compare.py` and `blind_compare.py`: `release_vs_real.png`,
`memo_vs_real.png` and `blind_grid.png` come out the same. The blind grid's key is
not in the repo.

## Not done

- T10 (worn elite) is about 30% light by dark pixels, and pica about 13% light by
  this measure: filed as a p3 issue.
