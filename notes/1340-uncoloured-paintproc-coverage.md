## A pattern painted inside an uncoloured PaintProc paints its coverage in every output (issue #1340, 2026-11-20)

Follow-up to #1300 (`notes/1300-patterns-across-contexts.md`). A mark
painted with a pattern inside an uncoloured (PaintType 2) pattern's
`PaintProc` came out three ways: the PNG took the inner pattern's
coverage, PDF filled the mark solid (an uncoloured stream takes no colour,
so the pattern selection was dropped), and SVG wrote the inner pattern's
own colours. A shading pattern there filled its whole shape in SVG and PDF.

## What gs does (10.07.1, png16m, 72 dpi)

Measured with four programs: a page pattern of red and white stripes, the
same made inside the `PaintProc`, an uncoloured page pattern (set in red),
and shading patterns made inside (one with a `/BBox`, one not extended).

- **The inner pattern's coverage, in the outer colour.** Wherever the inner
  pattern paints, whatever its colour: the white stripes count as much as
  the red ones, and no red or green shows. An uncoloured inner pattern's
  own colour is ignored the same way.
- **A shading pattern counts where it paints.** Inside its `/BBox`, and
  only between its ends when it does not extend (Coords `[10 0 30 0]` with
  no `/Extend` covers x 10 to 30 of a 0 to 40 rect).
- pscat's PNG already did all this: the capture paints the inner pattern's
  pixels into the tile and the uncoloured period is recoloured from its
  alpha. Nothing in the raster changes here.
- gs's own pdfwrite fills such a mark solid (it drops the inner pattern),
  so it is no reference for the PDF; its raster is.

## How

- **SVG** (`SvgRecorder::coverage_mask`). In an uncoloured tile, a mark
  painted with a pattern is drawn in `currentColor` (the colour the
  `<pattern>` sets per use) inside a `<g>` seen through an alpha `<mask>`:
  a rect over the capture canvas painted with the inner pattern. The group
  carries no `transform`, so the mask is in the recorder's coordinates even
  around a pen-space stroke, and a clip or blend that would sit on the
  element goes on the group. One mask per inner paint (an uncoloured inner
  pattern named in black: only the alpha counts).
- **PDF** (`PdfRecorder::coverage_mask`). The mark keeps the stream's own
  colour and is seen through `/SM<i> gs`: an ExtGState with an `/SMask`
  (`/S /Alpha`) whose form fills the pattern's `/BBox` with the inner
  pattern. The form's `/Matrix` is the capture matrix, which undoes the
  stream's opening `cm`, so the form's space is the pattern's own space:
  the one the inner pattern's `/Matrix` maps to, whichever way a reader
  takes a pattern inside a form. A pen-space stroke sets the mask before
  its pen `cm`.
- **The outer pattern is written coloured.** gs drops an uncoloured pattern
  whose resources hold a soft mask: the whole cell vanishes, plain marks
  too (measured by patching a pscat PDF by hand; an ExtGState with only
  `/CA` does not do it). The PDF spec also bars colour-related parameters
  from an uncoloured pattern. So an uncoloured pattern whose content holds a
  coverage mask is written as PaintType 1, its colour set at the start of
  its stream (`rg` and `RG`; its content sets no colour, so every mark,
  imagemasks too, takes it), in black and in a copy per colour it is used
  in (`colored_variant`), selected with `/Pattern cs` rather than `/PCS`.
  gs paints that with the soft mask exactly as it paints the program.
- Uncoloured patterns that paint no other pattern keep PaintType 2 and
  every byte they had.

## Limits

The new work is per mark and bounded by it: a mask def or form is made
once per inner paint and capture (deduplicated), each a few hundred bytes,
and a coloured copy once per pattern and colour, like the `/Matrix` copies
#1300 makes. The PDF's pattern streams, forms included, go through the
writer's budgeted deflate (`deflate_budgeted`, `--max-memory`). Nothing new
is rasterised, so nothing new is charged against `--max-steps`.

## Verification

`tests/patterns.rs`, against our PNG (mean block difference out of 255):

| picture | SVG via resvg | PDF via hayro | our PDF via gs at 8x | our PDF vs the program, both in gs |
|---|---|---|---|---|
| page and made-inside stripes (fill, stroke, pen-space stroke) in an uncoloured pattern, two colours | 0.21 | 0.04 | 0.12 | 0.43 |
| uncoloured page dots and two shading patterns made inside | 0.11 | 0.00 | 0.18 | 3.0 (see below) |

Plus: no pixel of an inner colour in the PNG, resvg's SVG, hayro's PDF,
gs's run of the program and gs's run of our PDF; the white stripes paint
(15 px of every 30); the two colours are two coloured copies; an
uncoloured pattern with no inner pattern keeps PaintType 2 and writes no
mask; an error after a coverage mask leaves no mask on the page.

In the second picture gs's PDF and PostScript interpreters put the edges
of the small whole-point dots a pixel apart (the PostScript one paints
them a pixel wider), so it is held as #1300's pictures are, through gs's
8x raster of our PDF. Our PNG against gs's 72 dpi run of it measures 3.8,
the same aliased-edge difference: no inner colour and no misplaced
coverage. The picture uses a whole-point outer offset, so the inner
lattice's anchor at a fractional one (#1342) is not what it measures.

Single-context and #1300 pictures are unchanged: `examples/patterns.ps`
and `examples/shading_patterns.ps` render PNG, SVG and PDF byte for byte
alike with the base binary (`feature/0.7.1` at 6c731811) and this
branch's, and the issue's programs differ in SVG and PDF (the control).

## Gaps

- In the PDF, objects made while a `PaintProc` that then fails ran (an
  inner pattern's copy, a mask's form) stay in the file unreferenced, as
  an inner pattern's copy already did.
- A viewer that ignores a soft mask inside a pattern paints such a mark
  solid in the outer colour, as every viewer did before.
