## drykit: a growing part keeps its drawn pixels; a /From cut starts flat (issue #1419, 2026-11-26)

psanim grows strokes with `/From 0 /To t`, and the one frame where the drawn part's edge
pixels moved showed as flicker. Incremental `/From prev /To now` pieces came out lighter.

**Cause (measured, not the chunk boundary).** A 180pt `dcchalk` line, frames 3.5% apart:
one transition (0.685 to 0.72) moved 493 pixels along the drawn part, by up to 29. Raising
`dkbudget` and emptying `dkZthin` left it unchanged; forcing `dkZink` onto its fill branch
removed it (and the compressed-charcoal shift #1418 could not explain, 237 pixels by up to
81, with it). `dkZink` strokes a constant-width run (round caps) and fills a varying one
(a disc per stop, a quad per segment); the two anti-alias an edge differently, and a cut run
switched branch in the frame where it first reached a change of width. A second, smaller
shift (up to 11 levels, a few dozen texture pixels) came from `dkZthin`, whose step is
worked out from the run's mean segment length and widest stop: the cut run's, so it changed
as the run grew.

**Fix.** In part mode (`dkSpart`), `dkFrun` gives every stop of a cut run a 4th slot, one
array shared by the run, `[flags hwmax canvaslength n]` of the WHOLE run (flags: 2 = the
whole run's width varies; 1 = cut at its start, on the first stop's own copy):
- `dkZink` takes the whole run's branch (fills when the whole run varies);
- `dkZthin` takes the whole run's step (widest stop, mean canvas segment length), so a
  growing part keeps the stops its catch was thinned to;
- `dkZgroup` sizes chunks with the whole run's widest stop, so chunk boundaries do not move;
- a run cut at its start is clipped flat (`dkZbehind`: an eoclip of a big box minus the
  half-disc behind the cut line, of the cut stop's own reach plus a device pixel: the
  previous piece's round end; an earlier draft took the widest reach of any stop, which
  notched a stroke whose width steps up just past the cut, review finding on PR #1441), in either branch, so the stroke branch's AA is unchanged. A first version
  took out a box 1.5 reach deep and wide, which notched the next leg of a piece that starts
  just before a corner; the half-disc is inside the previous piece's round end, so nothing
  it removes is lost (`a_flat_start_leaves_no_notch_at_a_corner`). Pieces of a zigzag and a
  box (300 pieces): weight 0.99 to 1.00 (before: 0.77 and 0.85). Ghostscript runs part mode
  (100 pieces and a chalk part) cleanly.
`dcchalk`'s tip strokes now hand drykit the whole polyline with `/From`/`/To` (instead of
cutting it with `dcpiece` and remapping `/Touch`/`/Pressure`), so they get all of the above
and share the whole stroke's stop grid. `/Tilt` side strokes still cut their lanes in
drychalk: that is #1420's lane (dropout chains), and a growing `/Tilt` stroke can still
shift a lane's edge once.

**Measured after** (`/From 0 /To t`, 21 frames 3.5% apart, 144 dpi): `dcchalk`, `dkstroke`,
`dhstroke /compressed` and `dycrayon` keep every pixel behind the pen's last 12 points,
through to `/To 1` (tests now assert 0). Incremental pieces, ink weight against the whole
(mean |pixel - bare| over the whole stroke's ink):

| pieces (length) | chalk before / after | dkstroke (w 6) | compressed charcoal |
|---|---|---|---|
| 2 | 1.00 / 1.00 (seam 4) | 1.01 / 1.00 (seam 102 to 26) | 1.00 / 1.00 (82 to 25) |
| 300 (0.2 widths) | 0.91 / 0.97 | 0.73 / 1.02 | 0.96 / 0.97 |
| 1500 (0.05 widths, under a device pixel) | 0.80 / 0.87 | 0.72 / 0.97 | 0.89 / 0.89 |

Tried and dropped: flat at both cut ends (worse: 1500 pieces 0.84 dkstroke, 0.82 charcoal,
seams conflate), a flat end one device pixel past the cut (helps 300 dkstroke pieces a
little, nothing at 1500), shifting the flat start a quarter point either way (no change).
At 1500 pieces every pixel straddles a cut, and each piece paints its levels through an
anti-aliased edge over the last piece's finished colour, which no PostScript-level overlap
avoids: the faint level shows through at the seams. So `/From 0 /To t` remains the exact
way to animate; incremental pieces are good down to about a fifth of a width.

**Not done.** A run long enough for drykit to draw in several chunks (thousands of points at
the shipped `dkbudget`) is thinned and inked per chunk when drawn whole (not a part), so its
`/To 1` frame may differ from the last part's (tested with a low `dkbudget`; the growing
frames before it agree). Making whole strokes use whole-run statistics per chunk changes
default renders, so it is a follow-up. The default path (no part) is byte-identical:
`scripts/identity_check.sh --loader lib/drykit.ps` (two binaries, a part-mode control).
