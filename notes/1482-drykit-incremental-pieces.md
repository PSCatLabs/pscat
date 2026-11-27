## drykit: incremental pieces run heavy at a low touch, and a part's far end (issue #1482, 2026-11-27)

The issue reported two differences between a stroke drawn as incremental
pieces (`/From prev /To now` on a kept canvas) and the same stroke drawn
whole: the pieces come out heavier at `/Width 8`, and any `/From` part
differs from the whole at the stroke's far end. The far end is fixed. The
weight is a pscat rendering property that no library change can remove
under this issue's constraints, so the guide now gives the measured numbers.

**The far end (fixed).** The issue guessed that the catch is thinned from the
part's first stop rather than the whole run's. That is not the cause. walkpath
always emits a subpath's exact end point. When the pitch divides the subpath's
length, that stop repeats the last pitch stop: 80 pt at `/Width 8`, pitch 4,
ends `[90 20 4.6] [90 20 4.6]`. The whole run keeps both. `dkFrun` dropped the
segment of no length (`dkFhi dkFlo sub 1e-9 gt`), so a part reaching the end
had one stop fewer. One stop matters because a varying-width run is inked as
one nonzero fill of a disc per stop, and pscat's anti-aliasing counts a
coincident disc again at its rim: a circle filled twice in one path differs
from one circle by 115 levels at 144 dpi, while gs gives 0. Dropping the stop
from the whole stroke's fill alone moves its far end by 39 levels. `dkFrun` now
keeps a segment of no length when the part is open and the stop lies in
`[dkFa, dkFb)`, or `dkFb` is 1. walkpath makes such a segment only at a
subpath's end, so a `/To` below 1 never holds it.

The line `10 20 -> 90 20`, `/Width 8 /Touch 0.5 /Pressure { 0.3 mul 0.85 add }`
(worst level and pixels over 16, against the whole, at 144 dpi):

| part | before | after |
|---|---|---|
| `/From 1e-9` | 39, 18 pixels at x 91-95 | 0 (byte-identical) |
| halves at 0.237, 0.537 or 0.81 | far end 39, plus the seam | the seam only (29 to 33), and at most one pixel off by 1 elsewhere |

The thinning start is a negative result. With the end stop kept, cuts that fall
off the pitch grid (0.237, 0.537, 0.81) leave nothing beyond the seam. So a part
thinned from its own first stop draws the same catch.

**The weight (not fixed, measured).** Total ink against the whole stroke, at
144 dpi, on the 180 pt line `10 30 -> 70 36 -> 190 24`, `/Seed 4`:

| touch | width | 50 pieces | 300 | 1500 |
|---|---|---|---|---|
| default (0.8) | 4 to 12 | 1.001 to 1.006 | 0.981 to 0.985 | 0.913 to 0.927 |
| 0.5 | 4 / 6 / 8 / 12 | 1.087 / 1.130 / 1.160 / 1.198 | 1.260 / 1.289 / 1.305 / 1.318 | 1.315 to 1.324 |
| `{ 0.4 mul 0.3 add }` | 4 / 6 / 8 / 12 | 1.072 / 1.104 / 1.123 / 1.151 | 1.212 / 1.231 / 1.239 / 1.248 | 1.236 to 1.264 |

Other touches at widths 6 and 8 (40 / 300 pieces): 0.3 gives 1.24 to 1.31 /
1.84 to 1.89; 0.6 gives 1.06 to 1.08 / 1.14 to 1.15; 0.7 gives 1.03 to 1.04 /
1.05; 0.9 gives 0.99 / 0.96. The media at their defaults (40 / 300 pieces,
widths 4 to 12): `dcchalk` 1.00 / 0.955 to 0.962, `dycrayon` 1.01 to 1.02 / 1.045
to 1.066, `dhstroke` 1.004 / 1.000.

**Where the extra weight is.**
- Nothing lands outside the whole stroke's ink, `/Fallout 3` included, so the
  dust is not involved.
- Most of the excess is in pixels that are faint in the whole stroke. At
  `/Touch 0.5`, width 8, 300 pieces: +0.22 on the fainter half of the inked
  pixels, +0.08 on the brighter half.
- `/Levels 1` makes it worse: 1.28 / 1.88 at 40 / 300 pieces. So the layering
  of levels is not the cause.
- `/Halo 0` changes it little: 1.11 / 1.27.

**The mechanism.**
- Each piece ends round at its `/To`, and its round end reaches about
  (half-width + halo) / piece length pieces ahead, so the next few pieces paint
  over it.
- In pscat the clip is an anti-aliased mask. Painting a colour again through
  the same partly covered tooth rim adds coverage. A low touch leaves small,
  sparse knuckles, which are mostly rim.
- Drawing the whole stroke over itself 2, 3 or 5 times gives 1.20, 1.28 and 1.34
  at `/Touch 0.5` (1.02, 1.03 and 1.04 at 0.8), which matches the pieces.
- Ghostscript clips hard-edged. The same library there gives 1.011 for the
  stroke drawn twice, and 1.002 and 1.012 for 50 and 300 pieces at
  `/Touch 0.5`, width 8.

**Why the library cannot fix it.** There are two shapes of fix, and each breaks
something the issue keeps:
- **Flat `/To` ends.** Every piece would end flat at its cut. That changes the
  tip of every `/From 0 /To t` frame (psanim's), which must stay byte-identical.
  Limiting it to pieces past 0 still leaves flat-to-flat seams, each a line of
  anti-aliased pixels shared by two clips.
- **Leaving the previous piece's round end out of a `/From a` piece.** The
  piece would skip the half-disc of radius half-width plus halo just ahead of
  its cut. A lone `/From a` part has no piece before it, so it would lose that
  ink from its start. A lone `/From 0.5` is documented and tested
  (`a_tilted_piece_keeps_the_whole_strokes_lane_dropouts`).

drykit cannot see whether a piece was drawn before, and a new key is out of
scope. `/From 0 /To t` each frame stays the exact way to grow a stroke.

**Unchanged.**
- Whole strokes never reach `dkFrun`. `scripts/identity_check.sh` with the
  drykit, drychalk, drycharcoal and drycrayon loaders found 13 of 13 page
  renders identical, and the control (a `/From 0.5` part on the 80 pt line)
  differs.
- 25 `/From 0 /To t` frames each, base library against head on one binary, are
  byte-identical. The strokes: `dkstroke` with a rising touch, with a touch
  crossing bands, with `/Pressure` on the 80 pt line, and with `/Lanes 4`
  dropouts on it; `dcchalk /Tilt 75`; `dycrayon`; `dhstroke`. Only the halves
  on the 80 pt line change.
- Keys, defaults and errors of every procedure are unchanged.
- Follow-up: pscat's anti-aliased fill of coincident subpaths in one nonzero
  path over-covers their rim, where gs does not.
