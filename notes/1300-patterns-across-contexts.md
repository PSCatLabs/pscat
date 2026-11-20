# #1300: patterns used across contexts

Follow-up to #1038 (`notes/1038-tiling-patterns.md`). A pattern is made
in a context: the page, or the capture of another pattern's `PaintProc`.
Before this, a pattern painted only in its own context: a page pattern
used inside a `PaintProc`, or one made inside a `PaintProc` and used on
the page after it, painted nothing in every output.

## What gs does (10.07.1, png16m, 72 dpi)

gs keeps a pattern's device matrix (pattern matrix times the CTM at
`makepattern`) and applies it unchanged in whatever device paints with
the pattern. Inside a `PaintProc` that device is the pattern accumulator:
a device-aligned bitmap whose origin is the top-left corner of the outer
cell's `/BBox` in device space, rounded to the nearest pixel. So:

- **A page pattern inside a `PaintProc` keeps its device size.** With the
  outer pattern at `[2 0 0 2 4 7]`, inner 10 pt stripes 3 pt wide came out
  10 px apart and 3 px wide, not 20 and 6: the outer pattern's scale does
  not reach the inner one. The issue's scope said to map the inner tile
  "through the capture matrix"; gs does not, and the acceptance is gs.
- **Its lattice starts at the accumulator origin.** Outer `/BBox`
  `[-13 -21 50 37]` at `4.3 7.6` on a 200 pt page: inner stripes at
  x = -9 + 10k and at the rows the inner matrix's `ty = 200` gives from
  y = 155. The page height is part of the inner phase (it is in the inner
  device matrix).
- **The corner is rounded, not floored.** Measured with a 50 pt inner step
  so the anchor reads off directly: corners at x = -22.6, -21.4, -21.8,
  -15.65 anchored at -23, -21, -22, -16; at y = 143.6, 142.8, 143.9,
  169.55, 170.45, 157.35 anchored at 144, 143, 144, 170, 170, 157. The
  corner is taken under the pattern's own matrix, translation unrounded
  and before the whole-pixel step adjustment: with the outer pattern at
  `4.3 7.6` and `/BBox` x0 = -13.1 at scale 1.5 gs anchors at -15
  (round(4.3 - 19.65)), not at -16 (round(4 - 19.65), the rounded
  translation). Using the adjusted scale instead broke two of sixteen
  measured cases; the unadjusted one matches fifteen, and the sixteenth
  (a 127.35 px step) is a pixel off. gs moves a pattern's cells so the
  `/BBox` corner lands on a pixel where pscat rounds the pattern origin,
  so single-context patterns can already differ from gs by a pixel at
  such steps (unchanged here).
- **Inside a turned pattern** (30 degrees) the accumulator is still
  device-aligned, so the inner stripes stay upright on the page inside
  the turned cells.
- **Made inside, used on the page.** gs 10.07.1 crashes on this with its
  default pattern cache ("Unrecoverable error", png16m and ppmraw).
  With `-dMaxPatternBitmap=1` it paints it, and by the same rule: the
  matrix the pattern got in the accumulator, read as page device
  coordinates (outer at scale 2: 20 px stripes 6 px wide, anchored at the
  outer origin less the accumulator origin). That run is the reference.
  gs's forward case renders identically with and without the flag.
  pdfwrite places such a pattern at the page's identity instead; pdfwrite
  rebuilds patterns at a high level and is not the raster's reference.

## How

- Each capture records `to_gs` (`pattern::capture_to_gs`): its canvas
  coordinates to gs's accumulator coordinates for it. That is the canvas
  to the parent's coordinates (the tile's shader after its origin), to
  the parent's gs coordinates, less the rounded `/BBox` corner. The page's
  is the identity, and nested captures compose.
- Each tile keeps the `to_gs` of the context it was made in. Used in a
  context whose `to_gs` is `H`, its placement is `H^-1 * to_gs * matrix`
  (`pattern::foreign_placement`). Used in its own context nothing changes:
  `Ink::Pattern`'s placement is `None` and every output is byte-identical.
- **PNG**: the period's shader is placed the same way; when the result
  still maps pixels to pixels (a signed permutation with a whole
  translation) it samples nearest, otherwise bilinear.
- **SVG**: the `<pattern>`'s `patternTransform` is the placement (in the
  capture recorder's coordinates inside a tile).
- **PDF**: a pattern's `/Matrix` maps to the default space of the stream
  that paints with it, so a foreign use gets a copy of the pattern stream
  (same content and resources) under its own `/Matrix`, made once per
  pattern and matrix (`PdfRecorder::pattern_variant`). Pages that paint
  only native patterns keep every object they had.

## Verification

`tests/patterns.rs`, each picture against our PNG (mean block
difference out of 255, or the share of pixels off by more than half the
range):

| picture | SVG via resvg | PDF via hayro | PDF via gs at 8x | against gs |
|---|---|---|---|---|
| page stripes inside a 1.5x pattern, and on the page | 1.93 | skipped | 0.78 | 0.12% of pixels differ from gs's run of the program |
| uncoloured page dots inside it (`/PCS` in the outer stream) | 0.61 | skipped | 0.34 | |
| page stripes inside a turned pattern | 0.16 | 0.03 | 0.57 | gs: our PDF vs the program 0.000 |
| made inside, used on the page | 1.79 | 0.05 | 0.34 | gs: our PDF vs the program 0.000 (`-dMaxPatternBitmap=1`) |

The first picture's pixel check is the alignment check: its content's
edges are whole pixels, where gs (which paints pattern cells aliased)
and pscat agree pixel for pixel, so only the outer cells' fractional
edges differ; the same render shifted one row differs in over 5%. Its
PDF is not held against gs's run of the program at 72 dpi: gs's PDF
interpreter paints a cell's whole-pixel rect edges a pixel heavier than
its PostScript interpreter does, with or without nesting (a plain page
pattern of the same stripes measures the same), so that 0.5 bound does
not apply there; at 8x it holds. resvg draws the 1.5x pattern's 4.5 px
stripes 4 and 5 px wide in turn, hence its 1.8-1.9; hayro resamples a
pattern inside a pattern stream (blurred, same placement), so the
first two skip it.

Single-context patterns are unchanged. With the base binary (built from
`feature/0.7.1` at 561581ab) and this branch's: the eleven earlier
pictures in `tests/patterns.rs` and `examples/patterns.ps` render PNG,
SVG and PDF byte for byte alike, the four new ones all differ (the
control), and the base binary against itself is stable on all fifteen.
`scripts/identity_check.sh` with `--base-bin/--head-bin` on
`examples/patterns.ps` (plus its default drivers): 20 of 20 pages
identical, the control (the first picture above) differing.

## Gaps

- ~~In SVG, a page pattern used both on the page and inside a tile writes
  its captured defs twice.~~ Fixed in #1343 (`notes/1343-svg-pattern-defs-once.md`).
- Inside a pattern turned off the device axes, the inner period is
  resampled twice (into the outer tile, then onto the page); gs draws the
  outer cell at a whole-pixel position per copy, pscat at its exact
  lattice position.
- ~~A pattern painted inside an *uncoloured* pattern's `PaintProc` (from
  the page or made there): the raster takes its coverage, as gs does,
  but the PDF stream of an uncoloured pattern takes no colour, so it
  fills solid there, and SVG writes the inner pattern's own colours.~~
  Fixed in #1340 (`notes/1340-uncoloured-paintproc-coverage.md`).
- pscat runs a `PaintProc` at `makepattern`, gs at first use, so a
  `PaintProc` that names a page pattern made after the outer
  `makepattern` is `undefined` in pscat and works in gs. This feature
  makes the difference reachable.
- At a 127.35 px outer step (outer scale 1.5, `/YStep 84.9`) the inner
  lattice is a pixel off gs's; sixteen other measured cases match.
  Single-context patterns at half-pixel steps can already differ in
  phase from gs (gs moves cells so the `/BBox` corner lands on a pixel,
  pscat rounds the pattern origin).

A three-level case (a page pattern inside Q's `PaintProc`, Q made inside
R's, R on the page with fractional translations) matches gs's raster
(0.3% of pixels, edges only).
