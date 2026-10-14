# #298: specimen polish (canvas weave, knife pulls, handscript 7)

Before/after panel: `docs/specimen_polish_298_compare.png` (left = before,
right = after), built by `notes/298-compare-panel.py BEFORE_DIR AFTER_DIR OUT`
from 2x renders of the base tree and this branch.

## Canvas weave (lib/paintkit.ps, `pjcatch`)

The knuckles sat on a ruled lattice (only a 0.15W nudge), as dash-shaped
0.8 x 0.42 ellipses alternating in a checkerboard. At 2x that read as a square
pixel grid. Changes, all pure functions of (seed, i, j), still no rand draws:

- `pjwander`: each column (warp) thread shifts its knuckles in x, each row
  (weft) thread in y, by smoothstepped 1D value noise along the thread (a
  lattice point every 3 crossings, hash slot 5), in -0.35..0.35 of a cell. A
  column of knuckles moves together, so the lattice reads as wavy threads.
- Per-knuckle size 0.8..1.2 (hash slot 4); rounder knuckles, 0.65 x 0.55.
- The top-fifth growth goes from 0.25 to 0.45. Without it the gaps the
  wander opens cost coverage near full touch (#220's promise). A 200pt
  square, /Touch 0.9 / 0.95 / 0.99: before 95.9 / 98.7 / 99.7%, wander
  with 0.25 growth 93.6 / 97.0 / 98.5%, final 96.5 / 99.1 / 99.8%.
- `pjrange` margin 1.25W -> 1.75W (0.65 x 1.1 x 1.45 x 1.2 + 0.15 + 0.35).
  The 600pt budget self-test still builds 244 x 244 cells.

Variants tried (aspect, wander period, amplitude): 0.72x0.5/4/±0.25,
0.8x0.42/4/±0.35, 0.65x0.55/3/±0.35 (chosen), 0.9x0.4/5/±0.3. The dash-shaped
ones kept a hint of the grid; the rounder, faster-wandering one read most
like woven thread.

drykit's `dkcatch` has its own knuckle geometry (same heights and selection,
ruled centres) and was left alone, so drykit renders are byte-identical: #415.

Timing (advisory, 144 dpi): paintkit_canvas_demo 0.76 s -> 0.89 s,
gallery/evening_stream 13.9 s -> 15.0 s.

## Knife pulls (examples/paintkit_loading_demo.ps only)

The snow pulls were straight, `/Jitter 0`, flat pressure, Coverage 0.95: hard
parallel edges, uniform width, a strip of tape. The specimen now bows each
pull slightly (a curveto) and sets `/Jitter 0.8 /Coverage 0.88 /Drag 0.5`.
`pktrowel`'s defaults are unchanged, so no other render moves. Tried and
dropped: a tapering /Pressure (narrowed the pulls to slivers) and /Jitter 2.5
to 4 (shredded edges that read as torn paper).

## Handscript 7 (lib/handscript.ps)

`/seven [400 [70 700 360 700 180 20]]` is one quadratic under the midpoint
rule, so the bar and the diagonal slur into a ')'. The corner point is now
doubled (the #351/#411 technique). The advance stays 400.

Consequences:
- The issue text put this fix in strokeglyphs and asked for handscript to
  stay byte-identical; the dispatcher scoped it to lib/handscript.ps instead.
- strokeglyphs copies handscript's glyphs at load (`sginit`) and has no
  `/seven` override on main, so its 7 changes too. This is what #410 asks
  for. strokeglyphs.ps itself is untouched.
- Two more jitter draws per 7, so in hs-write output every glyph after a 7
  draws differently (field_notes: the axis labels after the 7 and the
  caption). drykit/strokeglyphs board pieces seed per stroke, so in
  chalkboard_lecture only the 7 itself changed.
- Left alone: the 2 (#416). The pkwet banding in the canvas specimen: #417.

## Renders regenerated

`scripts/identity_check.sh` against a detached origin/main tree, one binary:
default pick (paintkit loaders) with a /Touch pktrowel control, and
`--loader lib/handscript.ps` with a "7" control; both controls differed.
Differs, regenerated (the base tree reproduced each committed PNG first):

- docs/paintkit_canvas_demo.png, docs/paintkit_loading_demo.png (2x)
- gallery/renders/evening_stream.png (weave)
- gallery/renders/chalkboard_lecture.png, gallery/renders/field_notes.png (7)
- docs/drymedia/strokeglyphs_specimen.png (1x, as committed),
  docs/drymedia/strokeglyphs_subscripts_greek_specimen.png (2x) (7)

All other paintkit/handscript/strokeglyphs/mathkit/drykit loaders are
byte-identical, including gallery/alpine_lake.ps and the mathkit specimen.
