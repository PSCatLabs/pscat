## dkstroke, dycrayon, dhstroke /From /To (issue #1418, 2026-11-25)

`/From a /To b` (0..1) on `dkstroke` draws that part of each subpath, with the whole stroke's
touch, widths, lanes, seed, texture and fallout. `dycrayon` (core and rim strokes) and
`dhstroke` (a wrapper) get it from drykit. Mirrors `dcchalk`'s keys from #1409
(`notes/1409-drychalk-from-to.md`): same arc-length meaning, same dot rule, same errors
(`dkstroke-from-must-be-0-to-1`, `-to-must-be-0-to-1`, `-from-must-not-exceed-to`; crayon says
`dycrayon-...`; `dhstroke` raises drykit's).

**Design: cut after resolving, in drykit.** dcchalk cuts the polyline before drawing and remaps
`/Touch`/`/Pressure` (its profile is its own). For drykit that cannot work: `/Touch`, `/Pressure`,
`/Wear`, the lanes' chains and the seed are resolved per stop from the whole subpath inside
`dkSsub`, and crayon's and charcoal's profiles are procedures of the subpath's `t`. So the
whole subpath is walked and resolved exactly as today, and the resulting runs are cut:
- with a part (`dkSpart`), a lane stop is a 4-element `[x y hw t]` (t = walkpath's progress,
  0.5 for a one-point subpath); `dkFrun` clips each run to `[From, To]` by `t`, adding an
  interpolated stop where a cut falls inside a segment, and returns plain `[x y hw]` stops;
  everything downstream (`dkZdraw`, thinning, splitting, fallout) is unchanged code.
- a one-stop run (a dot, or a lane on at one stop only) is whole or nothing: `From <= t < To`,
  `To` 1 closed (as dcchalk's dot rule). Empty or reversed ranges draw nothing; a range under
  1e-9 of the subpath is dropped, anything larger is drawn (a sliver is not lost).
- fallout: every particle is still placed (same counts and seeds as the whole stroke), and
  `/Mark` draws it only if its place along the chunk, mapped to the subpath's `t`, is in
  range. Chunks wholly outside are skipped.
- dycrayon forwards the keys to both its rim and core `dkstroke` (validated first under its
  own error names); its touch closure reads the whole subpath's `t`, so landing, fade, wander
  and build-up are the whole stroke's.
- dcchalk builds its own options dict (`dcSo`), so the keys do not double-cut a chalk stroke.
  `dhhatch`, `dhsketch`, `dhsmudge`, `dhlift` merge the caller's keys into their dkstroke
  calls and so apply the part to each stroke they draw; not stripped (documented).
- Defaults: `dkSpart` false runs the old code; identity-checked (`--loader lib/drykit.ps`).

**Better than chalk's cut, where it matters.** Pieces share the whole stroke's stop grid, so a
`/Tilt` piece's lane dropout chain is the whole lane's (chalk restarts it per piece), and the seam
is not a few tens of levels apart because of a re-walked grid.

**Measured** (1x and 2x pages, a 180pt curve, whole-stroke profiles): `/From 0 /To t` frames 5%
apart agree behind the pen's last 12 points to within 6-13 of 255 (dkstroke, crayon), except about one
frame in 14 on charcoal, where texture pixels along the whole drawn part change (below): up to 78, a few hundred pixels.
Adjacent halves on one canvas: worst 27-78 levels, a handful of pixels over 32 (compressed
charcoal: 45 of ~1100 inked). Cost (release, start-up included, a zigzag): whole vs a tenth,
dkstroke 0.24 s vs 0.09 s, dycrayon 0.45 vs 0.08, dhstroke 0.31 vs 0.07. The walk and profile
are still whole-stroke work; the catch and the ink are the part's.

**The occasional shift: cause not isolated.** At one step (`/To` 0.8 to 0.85, compressed charcoal) a few
hundred pixels change all along the drawn part, by up to 84 of 255. Ruled out by experiment: drykit's chunk
budget (`dkbudget` 150000 and 800000 give the same pixels) and the catch's thinning (`/dkZthin { } def`,
verified to take effect by a probe that empties it, gives byte-identical output). #1409 saw the same with dcchalk (whose cut is different), so it is probably in drykit's drawing
of a longer run, but that is not shown. Not chased further.

**Area limit.** `dkcatch-area-exceeds-safety-limit` is checked on the cut runs, so a growing stroke that the whole would refuse is accepted for the frames before the one that exceeds the limit.

**Proposed follow-ups.** (1) Find why a longer run re-textures the whole of its drawn part (not chunking, not thinning), and make a prefix stable. (2) Move `dcchalk` onto drykit's
`/From /To` (drops `dcpiece`, the touch remap and the lane-seed mixing; changes its partial output).
(3) Cap-less interior ends for sub-width incremental pieces (as in #1409).

**Update (#1419).** The occasional shift is explained and fixed: the cut run switched `dkZink` branch (stroke to fill) when it first reached a change of width, and `dkZthin` took its step from the cut run. Parts now use the whole run's branch, thinning step and chunk reach (`notes/1419-drykit-stable-growing-parts.md`).
