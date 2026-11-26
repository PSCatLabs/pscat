## TilingType 2 cells keep their size; the spacing varies (issue #1332, 2026-11-20)

Before: a TilingType 2 (constant spacing, "no distortion") pattern was
adjusted like types 1 and 3: its cells were scaled to the whole-pixel step.
A 7.3 pt step painted a 3.65 pt mark 3.5 px wide at 72 dpi. gs 10.07.1
keeps the cell undistorted and spreads the spacing instead.

## What gs does (10.07.1, `png16m`, 72 dpi)

- The cell is rendered once at its true device size and copied to every
  lattice position. Along a device axis with signed device step `S` and
  anchor `a` (the matrix translation rounded to a pixel), cell `i` starts
  at `a + floor(i * S)`. A 7.3 px step gives starts 0 7 14 21 29 36 43 51
  58 65 73 (gaps 7 7 7 8 7 7 8 ...). A negative step (the page's y flip)
  floors toward minus infinity: starts 0 -8 -15 -22 -30, so the gaps run
  8 7 7 8, not 7 7 8 as `-floor(i*7.3)` would give. A test that checked
  only x would miss a sign bug.
- Overlapping cells (a `/BBox` bigger than the step): the later copy, right
  and lower in device space, is on top, whichever way the pattern's axes
  point (`lattice_positions` sorts for it).
- At exact whole products gs is not plain `floor` in f64: with step 10.4,
  cells 45 and 85 land at 467 and 883 where `floor(i * 10.4)` gives 468 and
  884 (gs's own float noise at `i * step` equal to an integer). pscat adds a
  1e-6 nudge so 7.3 * 10 = 73 stays 73. This is a known difference of at
  most one pixel for those cells; the tests pick steps that avoid it.

## How

- `pattern::geometry`: for TilingType 2 with an axis-aligned matrix (also
  turned 90 degrees or mirrored), steps of at least 1 px, and a step that
  is not a whole pixel, the capture scale is the true device scale
  (`rs = step_px / XStep`, not `round(step_px) / XStep`), and a `Lattice` is
  recorded: the signed device steps, the device position of cell 0, and the
  transform that orients the capture canvas to the device.
- `geom.matrix`, which SVG and PDF write, is then the unadjusted matrix
  (true scale, translation still rounded to the raster's anchor), so a
  PDF viewer or browser places and sizes the cells itself, with
  `/TilingType 2` kept in the PDF dictionary and `width`/`height` the true
  step in the SVG `<pattern>`.
- PNG: `PatternTile::raster` lays the oriented cell out over the canvas
  being painted (`pattern::lay_out`: one row of cells, then that row at each
  row position, so the work follows the canvas's area) and returns it as a
  canvas-sized, identity-transformed `Repeat` shader source. It is cached
  by canvas size (and colour, for an uncoloured pattern). Positions are
  computed from the pattern's origin for every `i`, never by adding steps,
  so the lattice does not depend on where the canvas starts (not tested
  with a banded render).
- Limits (Codex round 1 on #1372): `PatternTile::lay` runs before a fill
  paints (`Gfx::lay_pattern`, from `fill_coverage_limited`, `stroke` and the
  stencil path; `pattern_source` and `raster` only read the cache). It
  returns a matching cached layout before checking anything, drops a
  layout for another canvas or colour before asking for room for a new one
  (one is held at a time), checks the real peak (canvas + row of cells +,
  for an uncoloured pattern, the recoloured cell) against the heap
  headroom and falls back to the period when it does not fit, charges the
  layout's work (pixels written, a step per 1024, added to the fill's
  work) and errors with `StepsExceeded` when that is over the steps left,
  and looks at the limits between cell copies. Stroke, `imagemask` and
  glyph painting (`show` and the family: `Gfx::fill_path_direct` and
  `paint_glyph` take the show's probe and return their work; `ShowCtx`
  collects it and the interpreter charges it, the show frame being stepped
  with the limits now in force as an image frame is) all go through
  `lay_pattern` under their own probes.
  `tests/pattern_type2_limits.rs` holds one case each.
- Fallbacks to the period (cells at their true size, step rounded to
  whole pixels, so the PNG drifts from SVG/PDF across the page): a
  canvas over `MAX_LATTICE_PIXELS` (2^24), a layout that would not fit
  `--max-memory` (`Gfx::lattice_canvas`, a read-only check: two canvases'
  worth of RGBA beyond what is live; the lattice is built lazily per
  tile, cached, and freed with it), steps under 1 px, and a pattern used in another
  context than it was made in (#1300's `foreign_placement`).
  A pattern turned off the device axes keeps the adjusted whole-pixel
  matrix in all three outputs, as before (gs's behaviour there was not
  measured).

## Checked against gs

- `tests/patterns.rs::type_2_cells_keep_their_size_and_the_spacing_varies`:
  a 3 pt cell on a 7.3 x 9.7 step with a translated matrix, plus an
  uncoloured one on 5.6 x 4.45. Our PNG against gs running the program with
  no anti-aliasing: under 0.05% of pixels differ strongly (0 in
  practice); our PDF in gs against the program in gs: 0.000.
- A sweep of 44 random and chosen cases (steps 2.2-15 in both axes, cells
  from 1 to 1.4 steps, matrices identity, translated, x mirrored, y flipped,
  turned 90 degrees, 180 degrees, scaled 2): 0 pixels differ from gs.
- The SVG and PDF are checked by their markup (`width="7.3" height="9.7"`
  on the `<pattern>`, `/TilingType 2` and the true-scale `/Matrix` in the
  PDF) and, for the PDF, by gs. resvg's raster of the SVG and hayro's of
  the PDF drift from ours by 5.3 and 5.4 block-mean on the test picture,
  as if they rounded a pattern tile to whole pixels (7.3 -> 7; inferred
  from that one number, not measured further), so they are not held against
  ours. The SVG's placement in a browser is unverified: Chrome could not be
  driven in this session (the browser tool timed out).

## Observations for #1342 (half-pixel lattice phase), fixed there

Fixed in #1342 (`notes/1342-pattern-lattice-phase.md`).

- The anchor stays `round(tx), round(ty)` (the shader's translation). gs
  appears to anchor at the rounded device corner of the `/BBox`, not at the
  rounded origin: for a 3.65 pt cell with a `[1 0 0 1 0.4 0]` matrix gs's
  starts equal those of translation 0, and for `[1 0 0 1 0.6 0.6]` x moves
  by one pixel but y does not (origin y 59.4 would round to 59, the BBox
  top 55.75 rounds to 56). pscat's PNG is one row off from gs in that
  last case. The lattice reads its anchor from `geom.shader.tx/ty`, so
  that fix moves Types 1, 2 and 3 together.
- A PaintProc's `rectfill` inside the capture of a 3.65 pt cell paints a
  partial edge pixel of 50% coverage (alpha 125), where the same
  `rectfill` on the page paints 65%. It is the same with whole-pixel
  steps, so it predates this change. gs paints the edge pixel solid (its
  pattern cells are aliased).
