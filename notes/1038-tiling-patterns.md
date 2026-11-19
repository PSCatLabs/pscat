# #1038: tiling patterns (makepattern, setpattern, /Pattern)

First part of #1038: **PatternType 1** (tiling) patterns, coloured
(PaintType 1) and uncoloured (PaintType 2), all three TilingTypes, in PNG,
SVG and PDF. Shading patterns (PatternType 2) and an `imagemask` painted
with a pattern are follow-ups.

## What a program can do now

- `dict matrix makepattern pattern`, with gs's errors: a missing key is
  `undefined`, a wrong type `typecheck`, an empty or inverted `/BBox`, a zero
  step, PaintType outside 1-2, TilingType outside 1-3 and PatternType other
  than 1 are `rangecheck`. The result is a copy of the dictionary with an
  `/Implementation` entry, as in gs.
- `/Pattern`, `[/Pattern]` and `[/Pattern base]` colour spaces (a device
  base), `setcolor` with a pattern (and an uncoloured pattern's components),
  `setpattern` (outside a pattern space it first sets `[/Pattern cs]` over the
  current space, as gs does even for coloured patterns), `currentcolor`
  (components, then the dictionary; `null` for the null pattern) and
  `currentcolorspace` (`[/Pattern [/DeviceRGB]]`, as gs prints it).
- `fill`, `eofill`, `stroke`, `rectfill` and the show family paint with the
  pattern. A pattern space with no pattern yet paints nothing (gs's null
  pattern). `sethsbcolor` now sets DeviceRGB, as the PLRM says, so it leaves a
  pattern space like the other device colour operators.

## How (`src/pattern.rs`)

- **Eager PaintProc.** `makepattern` runs the `PaintProc` once, through a
  `Frame::Pattern` continuation (never a recursive call into the machine),
  on the new dictionary. gs runs it lazily at first use; the PLRM allows
  either and the results are cached either way. A `PaintProc` that prints or
  reads changing state therefore runs at a different time than in gs.
- **Capture.** While it runs, `Gfx` paints into a capture: the page canvas,
  the SVG recorder and the PDF page state (content stream, resource lists,
  open clip group) are set aside and replaced by a tile-sized canvas, a
  recorder with prefixed def ids, and an empty stream. The graphics state is
  the current one inside a `gsave` (as a Type 3 glyph's), with the CTM the
  capture matrix and the clip the `/BBox`. Showpage, copypage and erasepage
  do nothing there, `setpagedevice` is `invalidaccess`, and an error or
  `stop` out of the `PaintProc` puts everything back (`Gfx::abort_pattern`).
  A live window keeps showing the page (`Gfx::page_pixmap`).
- **Whole-pixel steps.** The pattern matrix is adjusted so each step is a
  whole number of device pixels (TilingType 1 and 3 allow a pixel of
  distortion for this; gs does the same). With the pattern's axes along the
  device's, the raster maps the period pixmap one-to-one at a whole-pixel
  offset with nearest sampling: no seams, no blur. Turned off the axes, the
  period is resampled bilinearly. SVG and PDF are given the same adjusted
  matrix, so cells line up across the outputs instead of drifting by the
  rounding per cell (a 7.3 pt step at 72 dpi would drift ~20 px across a
  page otherwise).
- **Raster.** The capture canvas covers the BBox; it is folded into one
  period of the lattice (drawn at every whole step that reaches the period),
  so a BBox larger than its step wraps. Fills and strokes use it as a
  repeating `tiny_skia::Pattern` shader, with the graphics state's alpha as
  the shader's opacity. Uncoloured: the period's coverage is recoloured to
  the current colour (the last colour is cached).
- **SVG.** Per page, the tile's content is one group in the defs (captured
  elements under the capture-to-pattern matrix, clipped to the BBox), and
  each `<pattern>` is one `XStep` x `YStep` cell holding a `<use>` of it at
  every step that reaches the cell. Uncoloured content is `currentColor`, set
  by a `color` on the cell, one `<pattern>` per colour.
- **PDF.** A `/PatternType 1` stream, `/Matrix` the adjusted matrix into the
  page's default space (or the enclosing pattern's space), content the
  captured stream after a `cm` back to pattern space, with its own
  `/Resources`. Pattern objects are numbered after the fonts, so a document
  without patterns keeps every object number it had. Pages paint with
  `/Pattern cs /Pn scn`; uncoloured with `/PCS cs r g b /Pn scn` and a
  `/PCS [/Pattern /DeviceRGB]` colour space resource. An uncoloured
  pattern's stream holds no colour operators, colour images or shadings.
- The pattern registry (`Interp::patterns`) is keyed by the returned
  dictionary's address with a weak reference, so only a dictionary
  `makepattern` returned paints, and a dropped one frees its tile.

## Limits

A capture canvas or period is at most 16384 px a side and 2^26 px
(`limitcheck`), and both count against `--max-memory`. A BBox more than
2^16 periods' worth of copies is `limitcheck`.

## Gaps (documented, not silent misplacement)

- A pattern paints only in the context it was made in: one made on the page
  and used inside another pattern's `PaintProc`, or made inside one and used
  after it, paints nothing.
- Text output modes (`--svg-text`, `--pdf-text`) keep glyph outlines under a
  pattern.
- `imagemask` under a pattern still paints the solid colour (follow-up).
- An uncoloured `PaintProc`'s images and shadings are dropped; an imagemask in
  one keeps its own colour in SVG.

## Verification

`tests/patterns.rs`: a hatch filling a circle, one uncoloured pattern in two
colours, and a sheet with a 7.3 pt step, a BBox wider than its step, a
patterned stroke and patterned text. Each is rendered to PNG, SVG and PDF;
resvg's raster of the SVG, gs's raster of the PDF, and gs running the
program itself are each within a mean block difference of 3/255 of
pscat's PNG. Plus seam-freeness (every 8 px hatch tile identical), the PDF
holding a real `/Pattern`, gs's errors and read-backs, the null pattern, and
an error inside a `PaintProc` leaving the page as it was.
