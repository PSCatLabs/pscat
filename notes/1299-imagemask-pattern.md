# #1299: imagemask painted with a pattern

Follow-up to #1038 part 1 (`notes/1038-tiling-patterns.md`). `imagemask`
paints its stencil in the current colour, and the PLRM's current colour
includes a pattern. Before this, a stencil under `setpattern` painted
whatever RGB had been set before the pattern, in every output.

## Semantics, against Ghostscript 10.07.1

```
gs -q -dNOPAUSE -dBATCH -sDEVICE=png16m -r72 -g260x260 -sOutputFile=gs.png im.ps
```

with one 16 x 16 stencil (a frame and its diagonals) drawn four times
after `0 1 0 setrgbcolor`:

- under a coloured two-stripe pattern (`hatch setpattern`): the stripes
  show through the stencil, never the green;
- under an uncoloured dot pattern (`[/Pattern /DeviceRGB] setcolorspace
  0 0.5 0 dots setcolor`): dark green dots through the stencil;
- under the null pattern (`/Pattern setcolorspace`, nothing set): nothing
  is painted;
- in blue, as a control: a solid stencil.

The pattern stays where its matrix puts it on the page: the stencil cuts
it, it does not carry its own copy of the pattern. pscat's PNG, and gs's
raster of pscat's PDF, now show the same four squares.

The 5-operand `image` paints its own samples whatever the colour space:
gs paints `[/Pattern] setcolorspace ... image` normally, so only the
stencil case looks at the pattern.

## How

- **PNG** (`ImageCtx::rasterize_patterned`, `Gfx::fill_pattern_stencil`):
  the stencil's device coverage is sampled exactly as the solid-colour
  loop samples it (pixel centre, `cell_at`, the unit-square clip of the
  usual idiom), the clip mask folded in, into the canvas-sized coverage
  mask fills already reuse. The pattern then fills the image's device box
  through that mask with `paint_from`, the shader a fill uses, so
  placement, `setalpha` and the blend mode are a fill's. The solid-colour
  loop is untouched (#835 measured it).
- **SVG**: the stencil, opaque white where it paints, is a `<mask>` def;
  the image's footprint, a parallelogram in device space with no
  `transform`, is filled with the `<pattern>` through it. A solid stencil
  is still the coloured PNG it was.
- **PDF**: the `/ImageMask` XObject is painted after `/Pattern cs /Pn scn`
  (`/PCS cs r g b /Pn scn` uncoloured) in the same `q ... Q`, which PDF
  allows for a stencil mask. `PdfRecorder::image` takes the ink instead of
  an RGB.
- The null pattern, or a pattern from another context, paints and records
  nothing, and leaves a pending `showpage` erase alone, as a fill does.

## Verification

`tests/patterns.rs` (the `MASKED_*` pictures): a coloured pattern through
an upright and a 30-degree stencil, and an uncoloured one in two colours
(one stencil painting its 0s), each held against resvg's raster of the
SVG, hayro's of the PDF and gs's of the PDF at 8x (mean block difference at
most 3/255), and gs's raster of our PDF against gs running the program
(at most 0.5). Plus the null pattern in all three outputs and `image` in a
pattern space. Measured:

| picture | SVG via resvg | PDF via hayro | PDF via gs at 8x | gs: PDF vs program |
|---|---|---|---|---|
| masked hatch | 0.145 | 0.386 | 0.467 | 0.000 |
| masked dots | 0.163 | 0.290 | 0.660 | 0.000 |

The tests' resvg is built without raster-image decoding, which drew no
`<image>` at all (17.6 against our PNG, the stencils simply missing), so
`resvg_png` now hands each embedded PNG to resvg as vector art with the
same pixels (`png_as_svg`, one rect per run, crisp edges).

`examples/patterns.ps`'s sixth panel gains a heart stencil painted with
the 30-degree stripes; pscat's PNG, gs's raster of pscat's PDF and gs's run
of the file show the same heart.

## Gaps

- An imagemask inside an uncoloured pattern's `PaintProc` still keeps its
  own colour in SVG (unchanged from #1038).
- gs paints pattern cells without anti-aliasing at 72 dpi, so its own
  render of a turned stencil's stripes looks heavier; the 8x comparison is
  the one that holds.
