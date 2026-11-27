## drychalk /Tilt: pieces keep the whole lane's dropouts (issue #1420, 2026-11-26)

A `/Tilt` chalk stroke drawn as adjacent `/From a /To b` pieces showed a seam up to ~110/255
on a few percent of the ink: drychalk cut each lane itself (`dcpiece`), so drykit walked only
the piece and each lane's chance on/off dropout chain (`/Dropout`, `/Load`) started afresh
at the piece's start (since #1416 with a seed mixed with `/From`, so a lane that starts off
did not stay off in every piece). psanim grows strokes with `/From`/`/To`, and the seams
showed as flicker.

**Fix.** As #1419 did for tip strokes, `dcsidestroke` hands drykit each lane whole (after
the lane's ragged-end `dctrim`, so a lane is still cut by its own length) with `/From` and
`/To`. drykit walks the whole lane and cuts it in `dkFrun`, so the lane's seed, its dropout
chain, its stops and touch bands, its ink branch and thinning step are the whole lane's, and
a cut start is flat (`dkZbehind`). The From-mixed seed and the `/Touch`/`/Pressure` remap
went with it; `dcpiece` and `dcPspan` had no other caller and are gone. A zero-length lane
(a dot) is still whole or nothing in drychalk, as before.

**Measured** (200x60 board at 144 dpi, the 180pt bent line of `tests/drychalk.rs`,
`/Tilt 75 /Side 8` with `/Seed 2`, `/Seed 1`, `/Seed 8`, none; and `/Tilt 90 /Side 12
/Seed 5`). Max channel difference from the whole stroke, pixels over 64, ink weight:

| pieces | before: worst / >64 / weight | after |
|---|---|---|
| halves | 56 to 107 / 0 to 53 / 0.99 to 1.00 | 20 to 51 / 0 / 1.000 |
| thirds | 60 to 107 / 0 to 258 / 0.93 to 1.00 | 23 to 56 / 0 / 1.00 |
| 20 | 78 to 107 / 5 to 119 / 0.92 to 0.95 | 56 to 67 / 0 to 1 / 0.99 |
| 100 | 73 to 107 / 20 to 81 / 0.80 to 0.84 | 67 to 71 / 1 to 10 / 0.93 to 0.96 |
| 300 | 71 to 107 / 109 to 238 / 0.64 to 0.68 | 69 to 71 / 5 to 22 / 0.87 to 0.91 |

Growing `/From 0 /To t` (16 frames 5% apart, straight line, with and without `/Seed`): the
drawn part behind the pen's last 12 points used to move by up to 11 levels; now 0 in every
frame. The many-piece residue is #1419's: each piece paints an anti-aliased edge over the
last one's finished colour, and a side stroke's overlapping lanes are all cut at once.
`/From 0 /To t` stays the exact way to animate.

Cost: a tenth of a 8-repeat zigzag (`/Tilt 75`) renders in 0.08 s against 0.07 s before
(whole: 0.60 s): every piece now walks each lane whole, the trade #1419 made for tips.
Ghostscript runs a three-piece `/Tilt` file cleanly.

**Not done: a round start just past a cut (drykit, every medium).** The remaining seams
at some joins show as a faint one-pixel arc, about the stroke's half-width behind the cut
(clearest across a `/Tilt` mark's wide face lane). It is not the lanes: drykit splits a
lane's (or any stroke's) run where the touch band changes, and a run that starts at a band
change within one reach after the `/From` cut is not itself cut, so it keeps its round
start and paints its level back over the previous piece. Bare `dkstroke` shows it: `/Width
8 /Touch { 0.4 mul 0.3 add } /Seed 3`, a line from 10 to 90 at 432 dpi, halves cut at 0.47
or 0.50 (the band changes at 0.5) differ from the whole by up to 82, darker, 15 to 30 device
pixels behind the cut; cut at 0.40 or 0.53, by at most 33 (the usual round end ahead of the
cut). A fix has to respect paint order: there the whole stroke painted the new band's
start under the earlier run, which may not hold for a band change the other way, so
clipping every run behind the cut is not obviously right. `/From 0 /To t` is unaffected.

Default renders (no `/From`/`/To`) do not reach the changed code: identity-checked.
