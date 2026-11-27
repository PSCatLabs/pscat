## A pattern's /BBox clip no longer counts its edge twice (issue #1375, 2026-11-27)

Before: a `rectfill` inside a `PaintProc` that ended where the `/BBox` ended
(`/BBox [0 0 3.65 3.65]`, `0 0 3.65 3.65 rectfill`) painted its edge pixels at
alpha 125 of 255 (0.49) where the same rectangle on the page paints 166
(0.65). With a whole-pixel step (TilingType 1, a 7.3 step rounded to 7, so
the cell is 3.5 px) it was 64 where the page paints 128: the square of the
coverage, not the coverage.

## Cause

`Gfx::begin_pattern` clips the capture to the `/BBox` with the ordinary
`clip`, which anti-aliases (tiny-skia samples 4x4: 0.65 reads about 0.75).
The `rectfill` then multiplies its own analytic 0.65 by that mask value
(0.65 x 0.75 = 0.49). The shape's edge and the clip's edge are the same edge,
counted twice. A larger `/BBox` over the same `rectfill` paints 166, which
confirmed it (`/BBox [0 0 5 5]`). A shape running past the `/BBox` was
likewise cut at a fraction of its edge pixel.

## What gs does (10.07.1, `png16m`, `/TilingType 2`, 3.65 pt cell, 7.3 step)

- 72 dpi: the cell is aliased, its edge column and row solid (255), 4 px
  wide.
- 8x, box-averaged to 72 dpi: the edge column reads 191 (0.75, the aliased
  4th 1/8-px column), where the page's `rectfill` paints 166 and pscat did 125.

## Fix

The capture canvas is the `/BBox`'s pixel footprint (`pattern::geometry`
floors and ceils it), so its edge pixels belong to the tile.
`Gfx::snap_clip_to_pixels` (called at the end of `begin_pattern`) sets the
clip mask to 255 on the outward-rounded footprint's border rows and columns,
with a 1e-3 px noise guard so a whole-pixel edge does not grow a row. A
footprint narrower than the guard (under about 2e-3 px, at a pixel boundary)
would have collapsed to nothing, so an axis whose guarded range is empty but
whose footprint is not (`hi > lo`) falls back to the unguarded floor and ceil
(#1470; before it, nothing was snapped and such a cell painted nothing). The
interior was already 255. Shapes keep their own anti-aliasing: an edge-on
`rectfill` now paints what the page does, and a shape past the `/BBox` fills
the edge pixel solid and stops there (as gs's aliased cell does). Only the
mask changes: the `ClipNode` path (`clippath`, SVG and PDF export) is still
the `/BBox`.

## Limits

No new allocation or loop sized by pattern geometry beyond the perimeter of
the canvas (at most 4 x 16384 bytes written on a mask `clip` had just made);
the tile's pixel and memory checks (`begin_pattern`'s `budget_allows`, the
2^24/2^26 caps) are untouched. A `/BBox` of 8000.4 square under
`--max-memory 50` and `300` stops with `limitexceeded (memory)` and under
1000 renders, identical before and after.

## Checked

- `tests/patterns.rs`: `a_rectfill_to_the_bbox_edge_paints_the_coverage_the_page_does`
  (Types 1 and 2, every pixel of the cell against the page's rectangle),
  `a_rectfill_past_the_bbox_fills_the_edge_pixel_and_stops`,
  `a_thin_or_long_bbox_keeps_the_shapes_own_edge_coverage` (a 0.4 pt cell and
  a 2000.65 pt one). All three fail on the old code.
- SVG and PDF bytes of the test programs are identical before and after
  (they never used the raster clip); `clippath pathbbox` in a `PaintProc`
  still reads `[0 0 3.65 3.65]`.
- `tests/patterns.rs`, #1470: `a_sub_pixel_bbox_still_fills_its_pixel_when_a_shape_runs_past_it`
  (a `/BBox` 0.0004 pt wide: the issue's program painted 0 pixels before and
  now a solid column; the unit that fails on the old code) and
  `a_sub_pixel_bbox_keeps_an_edge_on_rectfill_s_own_coverage` (an edge-on
  `rectfill` there is a few thousandths of a pixel of ink on the page too, so
  it reads 0 both before and after: a companion that the snap does not turn
  a sliver of ink into a pixel, not a failing-before test).
- Identity (`scripts/identity_check.sh`, base and head release binaries,
  every tracked `examples/*.ps`, `gallery/*.ps` and `selftest/drivers/*.ps`,
  the corpus's pattern users `examples/patterns.ps` and
  `examples/shading_patterns.ps` among them): 212 of 212 pages identical
  (four `examples/hybrid/compare_*.ps` fail alike in both trees and are
  skipped), the control (the issue's Type 2 cell) differing. No tracked
  example, gallery or selftest-driver render moves: their tiling patterns use
  whole-unit `/BBox`es, where the clip mask already was 0 or 255. That is
  not true of every committed program: the `ACROSS` and `ACROSS_UNCOLOURED`
  programs in `tests/patterns.rs` (an outer `/BBox [-3.3 -2.6 70.4 61.3]` on
  fractional capture edges, with a `rectfill` past it) change their edge
  rasterisation, intentionally: against a binary built just before #1466, at
  400 x 400, 1922 and 2524 pixels differ (the edge pixels, which are now
  solid, with the shape stopping there). `ACROSS_TURNED` and `MADE_INSIDE`
  measured byte-identical before and after, by the same method (the CLI,
  `--page 400x400`, the binary built just before #1466; `MADE_INSIDE`'s
  outer cell is on the page, 750 non-white pixels in its corner rows).
  #1470's report listed `MADE_INSIDE` among the changed ones; on the PNG it
  is not.

## Gaps

- A `cliprestore` with no `clipsave` at its level inside a `PaintProc` and
  an inner `gsave` rebuilds the `/BBox` clip through `fit_clip` (the saved
  state's page size is not the tile's), which is anti-aliased again: an
  edge-on `rectfill` after it still reads 125 (probe: `{ pop gsave
  cliprestore 1 0 0 setrgbcolor 0 0 3.65 3.65 rectfill grestore }`). It
  predates this change and nothing in the corpus does it; proposed as a
  follow-up.
- A `cliprestore` with no inner `gsave` does not reach the `/BBox` clip at
  all: it restores the caller's saved clip, because the caller's state is
  saved before the `/BBox` clip is installed. `{ pop cliprestore clippath
  pathbbox 4 array astore == }` prints `[0.0 -0.35 612.0 791.65]`, the page
  rectangle in the capture's coordinates, where the same probe with
  `gsave` before the `cliprestore` prints `[0.0 0.0 3.65 3.65]`. It predates
  #1466: a binary built from the commit before it prints the same page
  rectangle.
- A PaintProc clip other than the `/BBox` (`clip` inside it) is still the
  anti-aliased mask, and its product with the shape's coverage is the usual
  approximation everywhere in pscat.
- #1373 (a rotated or foreign-context TilingType 2 in the PNG) is a different
  piece of code (the lattice in `pattern::geometry`) and not touched.
