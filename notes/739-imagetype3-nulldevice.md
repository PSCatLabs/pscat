## `nulldevice` and `ImageType 3` masked images (issue #739, 2026-10-25)

Found by ArtColony/fractal_clouds (a ray-marched landscape library): an
`image` with an `ImageType 3` dict raised `typecheck` and `nulldevice` was
`undefined`; gs runs both.

### nulldevice

- `GraphicsState::null_device` (so `gsave`/`grestore`, `save`/`restore` and
  `setgstate` scope it, as gs's device is). `Gfx::painting_off()` is the
  one test every marking entry point makes: it is `suppress_paint > 0`
  (the metrics-only Type 3 run) or the null device. Fill, stroke, shfill,
  glyph fill, `erasepage`, the SVG/PDF recorders (`svg_wanted`,
  `pdf_wanted`) and `ImageCtx::rasterize` check it. The image check sits
  in `rasterize`, after the data is read, so the data source is still
  consumed (a procedure still runs).
- It must come before `prepare_paint`, or a mark on the null device would
  consume a pending `showpage` erase and make the canvas a trailing page.
- `nulldevice` sets the CTM to identity and clears the clip (gs's
  `setdevice` does both); `base_ctm()` is identity on it, so
  `initmatrix`, `defaultmatrix` and `initgraphics` (also `showpage`'s)
  stay on the null device.
- `showpage`/`copypage` return before pushing a page, ending an SVG/PDF
  page or arming the lazy erase. Their `initgraphics` still runs.

### ImageType 3

- `ops::image::image_type3` reads `InterleaveType` (3 only: 1 and 2 are
  `limitcheck`, other integers `rangecheck`), `DataDict` and `MaskDict`,
  builds one `ImageCtx` per dict (`image_ctx`, the old `dict_image` body)
  and attaches the mask as `ImageCtx::stencil`. The two sources are
  filled one after the other (`fill`, `drain_filters`); `waiting`/
  `supply` route a data-proc result to whichever plane asked, so
  `interp.rs` is unchanged.
- The mask keeps its own Width/Height/Decode/ImageMatrix, so mask and image
  may differ in resolution and placement. Per device pixel the user-space
  point is mapped into each plane (`ImageCtx::cell_at`, the single
  point-to-sample seam #730's `/Interpolate` should replace); the pixel is
  painted when the image cell exists and the stencil lets it through.
- Polarity is pinned against gs 10.07: `[1 0]` paints the image where the
  mask sample is 1, `[0 1]` where it is 0, and a MaskDict with **no**
  Decode paints the 1s (unlike a plain `imagemask` dict, whose default
  `[0 1]` paints the 0s). `/ImageMask true` is accepted but not required;
  BitsPerComponent is forced to 1, as for `imagemask`.
- SVG and PDF: both take one straight-alpha RGBA raster
  (`ImageCtx::composite`): the image's cells expanded by an integer factor
  (at most 64 per axis and 16 Mpixel; the cap limits the expansion only, so a huge image is still emitted at 1x) so a finer mask is not lost, each cell
  gated by the same stencil test the rasterizer uses. SVG embeds it as a
  PNG with alpha; PDF writes an RGB image plus a DeviceGray `/SMask`
  (`pdf::ImageData::RgbA`; the soft mask is its own image entry, not a
  page resource, referenced through `PdfImage::smask` because object
  numbers are only fixed in `finish`). A stencil `/Mask` would need both
  planes to share one footprint; the composite does not.
- Source order matches gs (probed with two data procedures that print): the
  mask's source is read in full, then the image's. Two sources sharing one
  `currentfile` or one filter chain therefore see the mask bytes first.
- A MaskDict `/BitsPerComponent` other than 1 is `rangecheck`.
- Limits: no `InterleaveType` 1/2, no `ImageType 4`, the stencil does not
  extend the image's footprint (it only removes pixels), and
  `setalpha`/blend still do not reach images.

### Checks

gs renders of the issue repro and of a rotated, 8x8-mask-over-2x2-image
case match pscat's PNG (3 differing edge pixels of 80000 under rotation);
gs's rasterization of pscat's PDF matches pscat's canvas.
Tests: `tests/imagetype3.rs`, `tests/nulldevice.rs`.
