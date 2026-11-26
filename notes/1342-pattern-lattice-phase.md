# #1342: pattern cells sit where gs puts the /BBox corner

Follow-up to #1300 and #1332. pscat placed a pattern's cells from its
origin rounded to a pixel (`round(tx), round(ty)`). gs places them from
the device top-left corner of the `/BBox`, rounded, so at a half-pixel
corner pscat was a pixel off in x, in y, or both. It shows as a whole
pattern one row or column away from gs's: a stripe or dot grid shifted by
a pixel, and inside a nested pattern the inner lattice shifted too.

## What gs does (10.07.1, `base/gsptype1.c`, `gs_pattern1_make_pattern`)

Read from the source and checked against gs's raster (below):

1. `compute_inst_matrix` takes the `/BBox` to device space under the
   pattern's own matrix and `adjust_bbox_to_pixel_origin` moves the
   pattern by `round(corner) - corner` (`floor(v + 0.5)`), so the device
   top-left corner of the `/BBox` sits on a pixel.
2. Only for TilingType 1 and 3, with the step matrix axis-aligned (a
   quarter turn counts) and the device `/BBox` within half a pixel of the
   step on both axes (`ADJUST_AS_ADOBE`): the cell is scaled to the whole
   size `floor(bbw + 0.5)`, to 1/256 px short of it for steps over 2 px,
   and moved by `floor(o * (size - step) + 0.5)` whole pixels, `o` the
   `/BBox` centre in steps, so the centre stays about where it was. Then
   `fix_bbox_after_matrix_adjustment` rounds the scaled corner onto a
   pixel again. gs scales the pattern's own x axis by the device x ratio
   and its y axis by the device y ratio, so for a quarter-turned pattern
   the device x extent gets the device y ratio; the anchor follows that.
3. Cells sit at that corner plus whole (or, unadjusted, fractional)
   device steps. The pattern accumulator of a `PaintProc` starts at the
   same corner.

## How (`src/pattern.rs`)

- `geometry`, for an axis-aligned or quarter-turned pattern:
  `gs_translation` computes the device translation that puts pscat's own
  cell edge (its adjusted scale for Types 1 and 3, the true one for a
  Type 2 lattice) on gs's corner pixel. The translation is split into a
  whole-pixel shader translation and a sub-pixel shift of R, so the
  capture canvas's pixels stay device pixels and the raster still samples
  nearest. `geom.matrix` (SVG, PDF) carries the same translation, so the
  three outputs move together, and Types 1, 2 and 3 all go through it.
- `capture_to_gs` takes the accumulator corner under `geom.matrix` for
  such a tile (the corner after step 2), and under the pattern's own
  matrix for a tile turned off the axes, as before.
- No new loops, allocations or work: the change is arithmetic at
  `makepattern`. A shifted cell can be one pixel wider or taller, which
  `checked_side`, `checked_area` and the capture's `budget_allows` already
  cover.
- Unchanged: a pattern turned off the device axes (gs does not adjust
  those, and pscat still resamples from a whole-pixel period).

## Checked against gs

The measure: each cell inks the quarter of its `/BBox` touching the
device top-left corner, the page is filled, and the left and top edges of
the inked runs are compared with gs's aliased raster at 72 dpi. gs paints
cells with any-part-of-pixel fills, so its far edges can be a pixel
further out; its near edges are on pixel boundaries, and every near edge
of ours must be one of gs's. Also the best whole-pixel shift of ours
against gs's (0 when the phase is right).

| sweep (cases) | before: phase off | after |
|---|---|---|
| Types 1/3, `/BBox` = step (58: translations 0, 0.4, 0.6 in x and y, scales 0.8-2, mirrors, flips, quarter turns, fractional corners) | 31 | 0 |
| Type 2, cell smaller than the step (49) | 24 | 0 |
| Types 1/3, steps of 1.2-2 px (`step <= 2` branch, 10) | 2 | 0 |
| Nested: a 50 pt page pattern inside an outer pattern (31 random) | 5 | 2 (both the gap below) |
| Nested, outer cells gs moves by a whole pixel to keep their centre (12) | 4 | 0 |
| Types 1/3, cell not within half a pixel of the step (40): cell 0's corner | 17 of 37 off | 0 of 37 |

The last row checks only the corner of the cell at the anchor, because
of the gap below. Our PDF, read by gs, places the cells as gs's run of
the program does (six of the cases, Types 1, 2 and 3, a mirror and a
quarter turn: every edge the same): our matrix already has the corner on
a pixel and a whole step, so gs's adjustment leaves it there.

`tests/patterns.rs::cells_sit_where_gs_puts_the_bbox_corner` holds seven
of these (Type 2 at 0.4, Type 1 at y 0.4, a mirror, the centre shift, a
scale of 2, a quarter turn, the nested centre shift) to 0 edges off gs;
with the old rule its first case already fails (5940 of 8910 edges off). The unit test
`whole_pixel_steps_snap_an_axis_aligned_matrix` now expects the origin at
row 500, not 501, as gs paints it (cell tops at 486, 493, 500 on a 520 pt
page).

## Gaps

- **TilingType 1 or 3 with a `/BBox` not within half a pixel of its
  step**: gs does not adjust the scale at all; the cell keeps its true
  size and the cells go at fractional device steps (as for Type 2,
  `is_simple` false). pscat scales it to a whole-pixel step, so its cells
  drift from gs's across the page (18 of 40 sweep cases off by a pixel
  or more away from the anchor). The anchor cell now matches.
  This is probably also #1300's 127.35 px nested case: its outer cell
  is 58 pt tall on an 84.9 pt step; a reconstruction renders the same
  before and after this change, the anchor cell matching gs.
- A device `/BBox` within half a pixel of the step but rounding to
  another whole number (a 7.4 px step, a 7.6 px cell): gs's period is
  the cell's rounded size, pscat's the step's, so the cells drift from
  the anchor on. Not measured (the sweep's cells equal their steps).
- A pattern turned off the device axes keeps pscat's whole-pixel period
  (gs tiles it at its true step).
- Inside a pattern turned off the device axes the inner period is still
  resampled twice (#1300's gap), not touched here.
- gs's quarter-turn scale (the pattern's x axis scaled by the device x
  ratio) is followed for the anchor only; pscat's cell keeps its own
  scale.

## Identity

`scripts/identity_check.sh` with two release binaries (base built from
`feature/0.7.1` at 6c731811, head from this branch) over every tracked
`examples/*.ps`, `gallery/*.ps` and `selftest/drivers/*.ps`: 229 of 229
page renders identical (four `examples/hybrid/compare_*.ps` fail alike
in both trees and are skipped), the control (the issue's Type 2 cell at
`[1 0 0 1 0.6 0.6]`) differing. The corpus's only tiling patterns,
`examples/patterns.ps`, use whole-unit `/BBox`es at the identity matrix,
where the old and new rules agree.
