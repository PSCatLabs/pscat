## `setalpha` and `setblendmode` reach images (issue #766, 2026-10-26)

`image`, `imagemask` and `colorimage` now take the graphics state's alpha and
blend mode in PNG, SVG and PDF output. They used to paint opaque; that was the
documented gap from #47.

- **Raster** (`src/image.rs`, `blend_pixel`). Images blit straight into the
  pixmap, so they composite themselves, in premultiplied space with a real alpha
  channel (a translucent image on a `--transparent` page stays translucent).
  The formulas are tiny-skia's for Normal and Multiply, and a unit test pins
  them against an actual tiny-skia `fill_rect`, so an image and a `rectfill` of
  one colour agree. The arithmetic for the default (alpha 1, Normal) is the old
  code, untouched, including its forced opaque alpha byte: that is what keeps
  every existing render byte-identical (`scripts/identity_check.sh`, two
  binaries, over the corpus plus the image drivers).
- **SVG.** `opacity` on the `<image>` (`fill-opacity` does nothing on an
  image). The blend goes through `clip_wrappers`, so it lands on the outermost
  clip group, or on the `<image>` when there is no clip, for the reason in that
  function's comment.
- **PDF.** `gs_ref` selects the `ExtGState` inside the `q` before `Do`. `ca`
  and `BM` apply to colour images, stencils and images with an `/SMask`; an
  ImageType 3 mask multiplies with `ca`.
- **Driver.** `selftest/drivers/image_alpha.ps` (3 pages) covers Normal at 0.5
  for every image kind, Multiply, a Multiply image inside a clip, and the
  gsave/initgraphics resets. It is deliberately not `%%IdentityCorpus: always`:
  a file that exists only in the branch cannot be compared, and the five other
  image drivers must stay identical.
