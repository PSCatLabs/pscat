## A TilingType 2 lattice keeps its spacing in any context (issue #1373, 2026-11-27)

Before: a fractional TilingType 2 pattern (#1332) painted in a context it
was not made in painted from its whole-pixel period in the PNG, so its
cells drifted from gs, the SVG and the PDF across the page. A 3 pt cell on
a 7.3 x 9.7 step, made on the page and painted inside another pattern's
`PaintProc`: gs starts along a row 7 14 21 29 36 43 51; ours were 7 14 21
28 35 42 49. The same for one made inside a `PaintProc` and used on the
page. And one made and used inside the same y-flipped `PaintProc` (any page
pattern's capture, the page being y-flipped) floored its y steps in
pscat's capture coordinates, where gs floors them in its accumulator's:
gaps 10 10 10 9 where gs gives 9 10 10.

The issue's rotated case does not reproduce (measured on #1375: the FFT
period is 7.303 in both at 30 degrees).

## What gs does (10.07.1, `png16m`, 72 dpi, aliased)

gs keeps a pattern's device matrix and applies it unchanged in the device
that paints (#1300), and lays the cells out at `anchor + floor(i * step)`
there (#1332). For a capture that device is the pattern accumulator,
device-aligned: so the floors are taken in gs's device coordinates for the
painting context, whatever pscat's capture canvas orientation is.

## How

`PatternTile::lay` (and `raster`) take the painting context's map to gs's
device coordinates (`Gfx::here_to_gs`: identity on the page, the capture's
`to_gs` inside a `PaintProc`). When it and the tile's own `to_gs` are both
signed permutations with whole translations (`pixel_exact`):

- `pattern::lattice_in_gs` maps the tile's `Lattice` into gs's coordinates:
  the signed steps permuted, cell 0's pixel rectangle mapped (its corner is
  the anchor), the oriented cell turned to match.
- `canvas_in_gs` gives the canvas's box there; the cells are laid out over
  that box (`lay_out`, unchanged), and the layout's shader maps it back to
  the canvas (the inverse of `here_to_gs`, then the box's corner), sampled
  nearest.
- The layout is cached by canvas size, colour and the rounded
  `here_to_gs`, one at a time as before.

On the page both maps are the identity, so the page path paints the same
bytes as before (no turned cell is made). A context turned off the device
axes or scaled against gs's (an outer pattern rotated, say) still paints
from the period, as before. SVG and PDF are unchanged: they already wrote
the true step through `foreign_placement`.

## Limits

The layout is over the painting canvas, which in a capture can be up to
2^26 pixels; `MAX_LATTICE_PIXELS` (2^24) still falls back to the period
past that. `lay_out_cost` counts the positions without allocating
(`lattice_count`), adds the positions, a turned cell and a recoloured cell
to the bytes checked against `--max-memory`, and the positions looked at
to the work charged to `--max-steps`. `lay_out` polls the limits between
copies as before. `tests/pattern_type2_limits.rs`
(`foreign_layouts_obey_the_limits`): a 100 px cell on 1.1 px steps over a
1000 x 1000 capture stops at 1000 steps with the page blank (a Type 1 inner
pattern runs under the same cap); 200 px cells on 1.01 px steps stop at a
50 ms deadline with the page blank; a 2 x 8000 cell over a 1000 x 10
capture falls back to the period under 8 MiB and lays out with 200 MiB.
The steps stop also leaves the SVG and the PDF page's content as they were
without the fill. By hand (release build, 3 pt cells on 1.01 px steps
inside a `PaintProc`): a 5000 x 4000 pt outer cell (20M px, past the 2^24
lattice cap) paints from the period in 0.14 s; a 4000 x 4000 one (2^24) is
laid out in 0.24 s, and runs under `--max-steps 100000` and
`--max-memory 400`; `--max-memory 100` stops both at the outer capture
itself, as before.

## Checked against gs

`tests/patterns.rs::a_type_2_pattern_painted_in_another_context_keeps_its_spacing`
pins starts along one row and down one column, quoted from gs, for four
programs: the page's pattern in a `PaintProc`, the same with the outer
pattern turned 180 degrees and the inner translated, one made in a
`PaintProc` used on the page (`-dMaxPatternBitmap=1`, as `MADE_INSIDE`),
and one made and used in the same `PaintProc`. All four fail on the old
code. `an_uncoloured_type_2_pattern_keeps_its_spacing_in_each_colour_in_another_context`
paints an uncoloured page pattern in two colours inside the turned
`PaintProc` (a turned and recoloured cell, a layout per colour): each
colour on gs's lattice, 0 pixels differing strongly from gs (12435 before). Whole pictures against gs (pixels differing by more than half the
range): 0 of 160000 for each, where the old code differed in 12620,
14244, 12435 and 4464; gs's raster of
our PDF: 0 for each, before and after. With a 3.65 pt cell the starts match
gs in all four and 861 pixels differ, the same 861 as on the page (gs's
aliased edge pixel against our 0.65 coverage, #1375's note). A stroke and a
`show` in a `PaintProc` lay their cells on the same lattice as gs (the
remaining differences are glyph edges: gs's Helvetica is another font).

The SVG and PDF bytes of the four programs are unchanged. Cell 0 from the
SVG's `patternTransform`s lands where gs's lattice anchors (by hand: the
turned case at device (13, 591), the made-and-used case at (50, 277)).

## Gaps

- A fill stopped by a limit leaves its pattern's own PDF object in the
  document, unreferenced (its `PaintProc` ran before the fill stopped). It
  is the same on the page on the base, and invisible.

- An outer pattern turned off the device axes, or scaled so its capture
  is not pixel-aligned with gs's accumulator: the period, as before.
- #1389 (Types 1 and 3 whose `/BBox` is not within half a pixel of the
  step tile at the fractional step in gs: still open, a 3 pt Type 1 cell on
  7.3 gives 7 14 21 28 here and 7 14 21 29 in gs) and #1390 need
  `geometry()` to give those patterns a lattice. Once it does, this change
  lays them out in any context too.

## Codex round 1 on PR #1480

- **A, a far origin.** The cell's turn into gs's device
  (`lattice_in_gs`) was composed in f32. With the cell near 1e8 px,
  cancellation lost the small translation that keeps the turned cell
  inside its bitmap, so the page came out blank. The turn is now worked
  out in f64. gs paints 7,688 px at 1e7, 1e8 and -1e8; we paint 7,688,
  7,688 and 7,440 (the starts drift by f32 rounding there, as on the
  base). Past 1e8 gs loses cells itself (992 px at 1e9, 1,364 at -1e9).
  At -1e9 the lattice guard (1e9 px) sends the pattern to the period,
  which paints 25,800 px, as Type 1 and 3 do there (they are unchanged
  from the base at every translation tried).
- **B, an uncharged turn.** The turn and the recolouring are counted in
  the layout's work and charged with the copies, before any of it is
  done. The turn is copied pixel by pixel and looks at the limits every
  64 rows (`orient_cell_polled`), with the same pixels as the
  nearest-sampled draw. With no cell reaching the canvas, nothing is
  turned, recoloured or copied: the layout is the empty canvas.
- **C.** Tests for both, gs's starts for a 7.35 step, negative steps,
  overlapping two-colour cells and two nested `PaintProc`s (plain, and
  the middle turned), and a ladder of clocks instead of a 50 ms budget. A
  sub-pixel cell is not pinned: gs paints it aliased, we paint its
  coverage (#1375).
