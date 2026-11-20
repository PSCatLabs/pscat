# #1298: shading patterns (makepattern PatternType 2)

The other half of #1038 (`notes/1038-tiling-patterns.md`). `makepattern`
with `/PatternType 2` used to be a `rangecheck`. Now a shading can be the
current colour, in PNG, SVG and PDF. `fill`, `eofill`, `stroke`,
`rectfill`, the show family and `imagemask` all paint with it. Axial and
radial shadings (types 2 and 3) are supported, and so are function-based
ones (type 1).

## What gs does (10.07.1)

Probed with `{ dict matrix makepattern } stopped` and png16m renders at
72 dpi.

**Errors.**
- A missing `/Shading` is `undefined`, one that is not a dictionary
  `typecheck`.
- The shading's own errors come at `makepattern`, as `shfill` gives them.
  For example, `/Coords [0 0 1]` is `rangecheck` and no `/Function` is
  `undefined`.
- A PatternType of `2.5` is `rangecheck`.

**Accepted.**
- `PatternType 2.0`. A real with a whole value now works for type 1 too.
- Any `/ExtGState` value. It is not read: pscat has no ExtGState.
- A singular matrix, which paints nothing.

**`/Background`.** `shfill` ignores it, but a shading pattern paints it
wherever the shading does not reach, inside the `/BBox`.
- It shows past an end that does not extend, inside a radial's first
  circle, and everywhere for an axis that collapses (Coords
  `[50 0 50 0]`).
- An array of the wrong length is `rangecheck`, a non-number in it
  `typecheck`. A value that is not an array is ignored. A component out of
  range is clamped.

**Placement.** The pattern is fixed where `makepattern` made it. A shape
painted later under another CTM shows the part of the shading under it.
A component given to `setcolor` in `[/Pattern /DeviceRGB]` stays on the
stack: a shading pattern is coloured.

## How

`makepattern` keeps the parsed shading, its `/Background`, and its
device matrix (the dictionary's matrix times the CTM) in a
`pattern::ShadingPattern`. Nothing runs and nothing is captured.
Tiling and shading patterns share the registry and the graphics state's
`PatternColor` as `pattern::PatternRef`.

**PNG.** Each mark builds a layer the size of the pixels it covers:
- the union of a fill's coverage runs;
- a stroke's path box, grown by its reach;
- an imagemask's footprint.

`gfx::shading_layer` paints the background, then the shading, both cut
to the `/BBox`. It is pure: it takes the shading, a matrix and a rectangle
and returns pixels, and never touches `Gfx`. It is built from `shfill`'s
own pieces:
- `gradient_parts`: the shader construction lifted out of
  `shfill_limited`, unchanged;
- `two_root_layer`;
- `function_layer`.

The mark uses the layer as its shader, sampled nearest at a whole-pixel
offset, under the mark's own coverage, clip, alpha and blend.

**Limits.** Every mark builds its layer under its own limit probe, before
anything is recorded or painted, and the operator charges the layer's
work: a step per 1024 units.
- **fill** and **stroke**: `Gfx::stroke_limited` is new for this, used by
  `stroke` and `rectstroke`.
- **imagemask**: the layer is built over the image's footprint before
  its SVG and PDF are recorded.

The layer's memory is checked against `--max-memory` before it is
allocated. So a page-sized layer stops `imagemask` and `stroke` at
`--max-steps` and `--max-memory`, as it stops `rectfill` (Codex round 1
on PR #1367).

Two later fixes (Codex round 2 on PR #1367):
- **A layer costs its pixels even when the shading paints nothing.** An
  axis that collapses, with a `/Background`, fills the whole layer. The
  allocation, background and mask work is charged on every path,
  including every early return.
- **An imagemask checks its layer against the steps left at that
  moment,** not against the probe taken when the image began. The image
  frame steps with the limits now in force, so steps its data-source
  procedure spent count.


**SVG.**
- A gradient with no `/BBox` and no `/Background` is the
  `<linearGradient>`/`<radialGradient>` itself, as `shfill` writes it.
  Its `gradientTransform` is the pattern's matrix.
- Anything else is a `<pattern>` one canvas in size, in device space. It
  holds the background, the gradient or the Type 1 layer as a PNG image,
  cut to the `/BBox`. Only the canvas can show a mark, so the cell never
  repeats anywhere it is seen.

**PDF.** A `/Type /Pattern /PatternType 2` dictionary, with no stream:
- `/Shading` points at a shading written as `shfill` writes it, plus
  `/BBox` and `/Background`;
- `/Matrix` maps the shading's space to the page's default space;
- a page paints with `/Pattern cs /Pn scn`.

A shading that paints nothing and has no background has no PDF form, and
its marks are left out, as the raster leaves them unpainted. One that
paints only its background becomes a constant axial.

**Contexts.** Following #1300's rule, a page shading pattern used inside a
`PaintProc` keeps its device matrix, read in gs's pattern-accumulator
coordinates (`pattern::context_hop`). The raster layer and the SVG
`<pattern>` go through the hop, and PDF gets a copy of the dictionary
under the new `/Matrix`.

**Pages of another height.** A pattern reused on a later page whose height
differs keeps its device placement, as in the PNG and in gs. The PDF
`/Matrix` maps to the page's default space, whose flip the height moves,
so that page gets its own copy of the dictionary. The check compares
against the `/Matrix` written at `makepattern`, not the context id
(Codex round 1 on PR #1367).

## Verification

Measured with `tests/patterns.rs`. Each number is the mean block
difference against our PNG, out of 255; the bound is 3.

| picture | SVG via resvg | PDF via hayro | our PDF via gs | the program in gs | our PDF vs program, both in gs |
|---|---|---|---|---|---|
| axial pattern filling text (the issue's check) | 0.11 | 0.09 | 1.15 | 2.01 | (fonts differ) |
| radial + Background, Type 1 + BBox, stroke, collapsed axis | 0.06 | 0.02 | 0.85 | 0.86 | 0.017 |
| imagemask with a shading pattern | 0.11 | 0.33 | 0.42 (8x) | | 0.000 |
| page shading pattern inside a tiling PaintProc | 0.54 | | 0.38 (8x) | 0.27 | |
| pen-space strokes (direct gradient and `<pattern>`), radial + BBox, Type 1 Background outside its Domain | 0.04 | 0.02 | 0.93 | 0.93 | 0.002 |
| radial with circles not nested (two roots) | (SVG gap) | | 0.21 | 0.21 | |
| `setalpha 0.5` fill | 0.14 | (hayro ignores `ca`) | 0.38 | (`setalpha` is pscat's) | |

**Text.** gs paints text under a shading pattern without anti-aliasing,
so the text picture is further from it at 72 dpi.

**The program against the PDF.** gs's run of the program draws its own
Helvetica, so for the text picture that comparison measures fonts, not
patterns. The other pictures hold it at 0.5.

`examples/shading_patterns.ps` is the specimen sheet: text, a radial sun
on a background cut to its box, circles from a colour table, and a
stroke and an imagemask. Next to gs's render of the same file it differs
only in gs's aliased edges.

## Gaps

- **Foreign SVG.** A Type 1 pattern's SVG image covers the made context's
  canvas only. Used in another context, it shows nothing past that.
- **Not-nested radials in SVG.** A radial whose circles are not nested
  keeps `shfill`'s SVG gap: SVG has no smaller root.
- **Text output.** `--svg-text` and `--pdf-text` keep glyph outlines under
  a shading pattern, as under a tiling one.
- **Mesh shadings.** Types 4–7 are not implemented, so neither are their
  patterns.
- **Uncoloured `PaintProc`.** A shading pattern used inside an uncoloured
  `PaintProc` paints its coverage in the raster and in SVG
  (`currentColor`), as gs does.
