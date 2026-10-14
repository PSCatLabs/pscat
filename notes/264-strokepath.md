## `strokepath` (issue #264, 2026-10-13)

`strokepath` replaces the current path with the outline `stroke` would paint.
Found while building the drykit surface-cost prototype, which had to grow the
path's bbox by half the line width instead.

- **Shared parameters.** `Gfx::stroke_params` now builds the tiny-skia `Stroke`
  (width, cap, join, miter limit, scaled dash) for both `stroke` and
  `Gfx::stroke_path`, so the two cannot drift. `stroke_path` runs what
  tiny-skia's `Pixmap::stroke_path` does before rasterizing: `Path::dash`, then
  `Path::stroke`, at resolution scale 1 (our paths are already device space).
  It starts from `to_skia_for_stroke`, so the tiny-closed-curve flattening from
  #169 applies identically. The result goes back through `PsPath::from_skia`
  (quads become cubics) and lands in `state.path`; as everywhere, it is device
  space, so `pathbbox` and `pathforall` report it in the CTM at the time of
  asking.
- **Empty and degenerate paths** leave an empty path and raise no error (pinned
  against gs: a following `pathbbox` is `nocurrentpoint`). A lone moveto and a
  zero-length subpath under butt caps are the same. Round caps make a dot.
  Square-cap zero-length subpaths are an exception: gs draws nothing, pscat a
  square, as `stroke` already did (the PLRM leaves it device dependent).
- **Fix found on the way: odd dash arrays.** tiny-skia rejects odd-length dash
  arrays, so `[3] 0 setdash` silently became a solid line. The PLRM repeats an
  odd array through two periods, which is the array doubled; `stroke_params`
  does that (SVG and PDF recorders get the doubled array too). gs now agrees
  pixel for pixel on `[10]` and `[10 5 3]`.
- **gs comparisons.** gs applies stroke adjust by default, which shifts stroke
  edges up to half a pixel (a 20-wide line at y=100 reports y 89.5..109.5).
  pscat has no `setstrokeadjust`, so the tests prepend `false setstrokeadjust`
  on the gs side only. With it off, `pathbbox` after `strokepath` matches gs to
  better than 0.01 for caps, joins, miter limit, dashes, curves, arcs, scale and
  zero width. The one exception is a round join: gs reports the control polygon
  of its Bezier arcs (about 1 unit beyond the arc at width 20), pscat the tighter
  box, so the bbox tolerance is 1.0. Pixel-wise, gs does not anti-alias, so
  slanted edges differ from pscat by about a pixel of grey; `strokepath fill` is
  no further from gs than pscat's own `stroke` is, which is what the test pins.
- **Deviations kept.** Width is the scalar sqrt(|det CTM|), so a non-uniform
  `scale` gives a round pen where gs has an elliptical one (already the case for
  `stroke`). Zero width is one device pixel centred on the line (gs: one pixel
  to one side under stroke adjust, centred without it). `pathbbox` itself differs
  from gs under rotation, with or without `strokepath`: filed as #288, and the
  rotated case is therefore compared by raster only.
- **Hairlines.** Widths of one device pixel or less take tiny-skia's hairline
  rasterizer in `stroke` but are an ordinary filled outline after `strokepath`,
  so the own-raster test skips the zero-width case and the gs test covers it.
- **Blast radius of the odd-dash fix.** No checked-in program uses an odd dash
  array (grepped `lib/`, `gallery/`, `examples/`, `site/`, `web/`, `art/`, the
  corpus; `lib/paintkit.ps` only sets `[]`), so no checked-in render changes. SVG
  and PDF output already repeated odd arrays themselves (both formats define it);
  only the raster was wrong, and now all three agree.
- **Known deviation: the current point.** After `strokepath` the current point
  is the start of the outline's first contour. gs starts on the other side of the
  line (butt line `10 10 moveto 50 10 lineto`, width 10: gs (10, 5), pscat
  (10, 15)), because tiny-skia's stroker walks the outline in the opposite
  direction. Not fixed: nothing in the repo reads `currentpoint` after
  `strokepath`, and the outline's winding and contents are unaffected.
- **CI has no Ghostscript**, so the live gs comparisons skip there; the
  hard-coded gs-pinned asserts still run. The gs comparisons were run locally
  against Homebrew gs.
