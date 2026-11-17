<!-- cloud: replace-file -->
# Slow renders

Read this when a render is slow or fails on the time limit. Back to
[SKILL.md](../SKILL.md).

The hosted server has no `bands` option and no profiler: each render runs on one
thread under the time limit that `get_renderer_info` reports, and a render that
passes it fails. What helps instead:

- Lower the `dpi`, or the page size. Cost grows with pixels, so a draft at
  `dpi` 36 or 72 is a fraction of the cost of one at 144, and the program does
  not change.
- Find the slow part by running `validate_postscript` (it uses the same limits)
  on the program with sections removed.
- Work per pixel is what usually costs: an `image` or `colorimage` procedure that
  calls `fbm2` or `ridged2` for every pixel, or a loop with many thousands of
  small fills. Lower the pixel count of the image and scale it up, cache values
  you compute twice, and draw fewer, larger marks.
- A program that guards its use of `/BandBBox` with `/BandBBox where { ... } if`
  still works: the key is absent here, so the guard does nothing and every row is
  painted. A program that reads `BandBBox` without that guard fails with
  `undefined`.
