# 730: honour `/Interpolate true` on images

`image` with an image dictionary (and `ImageType 3`'s `DataDict`) now smooths
a magnified colour image when the dict says `/Interpolate true`. Without the
key (or with `false`, a non-boolean, or on `imagemask`) nothing changes: the
output is byte-identical (PNG, SVG and PDF).

## Sampling rule (PNG)

- Sample `(i, j)` has its centre at `(i + 0.5, j + 0.5)` in ImageMatrix space.
- A device pixel centre is mapped back to that space; the result is the
  bilinear blend of the four nearest centres, in the decoded RGB (so Indexed
  and Separation images blend the colours they look up, not the indices).
- Edges clamp: a position outside the outermost centres takes the edge
  sample exactly. There is no fade to the page. The footprint is unchanged,
  so the first half-cell at each edge is flat.
- Only a *magnified* axis is smoothed: device pixels per sample along the
  image's x (or y) axis above 1. A minified axis keeps the nearest sample,
  so a downscaled image with the key renders as it did before. Rotated and
  skewed images blend along their own axes.
- `ImageCtx::cell_at` still decides which points are inside and which cell
  they fall in (the stencil gate uses it too); `smooth_rgb` only replaces
  the colour, so ImageType 3's mask edge stays hard.

Checked against Ghostscript 10.07.1 on the issue's repro (8x4 to 100x100 at
72 and 144 dpi): worst channel difference 4 of 255, mean 0.44. Without the key
the same comparison differed by up to 45. gs's filter is a hair softer near
the ramp ends.

## Other outputs

- **PDF:** `/Interpolate true` on the colour image XObject. Not on masks, and
  not on an ImageType 3 soft mask (the raster keeps that edge hard). The
  stenciled composite is itself built with bilinear colour at its own
  resolution (`composite()`), so PDF and SVG viewers see the PNG's look.
- **SVG:** `image-rendering="auto"` (the viewer's smooth scaling) when set;
  otherwise the existing `pixelated`, byte-for-byte. Documented gap: on a
  stenciled (ImageType 3) image the viewer smooths the alpha plane as well,
  so the mask edge softens by about one composite cell in SVG, where PNG and
  PDF keep it hard.

## Not done

- `imagemask` interpolation (gs smooths a mask into alpha): the key is ignored.
- Level 1 `image` and `colorimage` (operand forms) cannot carry the key;
  `colorimage` has no dictionary form, so the Level 2 dict `image` under
  DeviceRGB/CMYK is the colour path this covers.
- Smoothing Separation images through their tint transform: they keep the
  existing gray approximation and are blended in gray.
