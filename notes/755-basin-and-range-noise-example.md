## Basin and Range: a band-aware example for the native noise operators (issue #755, 2026-10-25)

`gallery/basin_and_range.ps` is a 720x480 shaded-relief map, drawn by one
`colorimage` data procedure from `noiseseed`, `fbm2` and `ridged2`. No library
is loaded, so it is a playground piece.

**Shape of the program.** Height = a broad `fbm2` (land and sea) plus a
`ridged2` crest term gated by a slow second `fbm2`, so the ranges form belts. A
256-entry colour ramp (three strings, built once from stops) turns height into
colour; the shade is `1 + gain * (dzx + dzy)` from backward differences (the
left sample in the same row and the previous row's height, kept in an array),
so each sample costs one `fbm2` and one `ridged2`. Contours every 0.04 of
height are anti-aliased by the distance in samples (`fraction distance /
gradient`), skipped on the coast itself, where they read as a dotted outline.
Peaks above 0.8 are eased toward 1 instead of clamped: a clamped plateau has
zero gradient, and the contour term then paints it a flat dark grey.

**Band-aware, and why in device space.** `/BandBBox` is in default user space
of the page pscat was given, and the committed still is drawn with `2 2 scale`
prepended. Comparing BandBBox with image-space y directly (the pattern in
`tests/bands.rs`'s `FBM_AWARE`, which is only run unscaled) would skip the
wrong rows under that prefix. The piece takes BandBBox's lly and ury through
`matrix defaultmatrix transform` and each sample row's two edges through the
image's own CTM, and compares device y. The margin is two sample rows in
device pixels: one for rounding, one because a row's lighting reads the row
above (a skipped row leaves the previous-row array stale, but only rows outside
the band are ever stale-lit).

**Evidence** (release build, 10-core M-series laptop, shared with other agents;
median of 5 interleaved runs after a warm-up):

| render | plain | `--bands 4` | `--bands 8` |
|---|---|---|---|
| `--page 720x480` | 1.69 s | 0.60 s | 0.45 s |
| 2x still (`2 2 scale`, `--page 1440x960`) | 1.92 s | 0.67 s | n/a |

`cmp` of the plain and `--bands 4` PNGs is clean at both scales. The sample
grid is fixed at 720x480, so the 2x still costs the same as 1x.
