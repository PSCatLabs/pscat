# #254: pdf export, zero-length `l` in fills and clips

`gallery/alpine_lake.ps` exports a 74 MB PDF after #244's Flate compression.
This note records what was measured and what shipped.

## What shipped

`PsPath::to_pdf_ops(area)` (src/gfx.rs) drops a `l` to the point the subpath is
already at when the path is filled or used as a clip, and more real geometry
(`l` or `c`) follows. Strokes keep it: a zero-length segment decides caps and
joins. Equality is judged on the printed (3-decimal) coordinates, i.e. on what
gs would read. A bare `m` + `l` with nothing after it is left alone.
`pkspray` specks are `m x y l x y` + four `c`, which is where it matters.

Test: `zero_length_lineto_is_dropped_from_fills_and_clips_but_kept_in_strokes`
in tests/pdf.rs.

## Verification

Binary built from origin/main (`dc399b6`) vs this branch, all 103 tracked
`examples/*.ps` + `gallery/*.ps` exported with `--pdf`, rasterised with
`gs -sDEVICE=png16m` (72 dpi examples, 144 dpi gallery, as
scripts/identity_check.sh), PNGs compared byte for byte: **103/103 identical**.

PDF size, biggest files (bytes):

| file | before | after |
|---|---:|---:|
| gallery/alpine_lake.ps | 74,372,818 | 72,699,691 (-2.2%) |
| examples/paintkit_wet_pull_demo.ps | 25,470,465 | 25,066,895 (-1.6%) |
| gallery/first_rain.ps | 1,825,569 | 1,752,851 (-4.0%) |
| gallery/fugitive_pigments.ps | 1,860,355 | 1,791,861 (-3.7%) |
| examples/paintkit_spray_demo.ps | 1,431,593 | 1,371,623 (-4.2%) |

All 103 files together: 243.13 MB -> 240.46 MB (-1.1%).

The gain is small because the dropped `l` repeats the preceding `m`'s two
numbers, which Flate already codes as one short back-reference.

## Where the 72 MB is (alpine_lake)

Inflated content stream 258 MB; compressed in two halves of about equal size:

- 200 `pkspray` fills holding ~600k specks (139 MB raw, ~37 MB compressed);
- ~166k single-subpath fills (118 MB raw, ~37 MB compressed) that are long
  near-straight stroke outlines: 4.9M `l` operators.

Opaque (no ExtGState at all in this file), nonzero, no overlap trouble, but the
file is mostly *digits*, which Flate cannot shrink.

## Measured and not shipped

- **Fewer decimals** (`f()` writes 3; tried 2 for path coordinates only, trailing
  zeros trimmed): alpine_lake 74.4 -> 58.8 MB (-21%), first_rain -17%,
  wet_pull_demo -24%. Rasters are no longer identical: 102 of 103 files differ
  by a few hundred to a few thousand pixels (alpine_lake 4,171 px, max channel
  delta 231 — gs draws without anti-aliasing, so a 0.005 pt shift flips edge
  pixels outright). Worst 10 px block-mean difference over the corpus is 0.10
  (the golden bar in tests/golden.rs is 6.0). It does not fix the issue (still
  ~59 MB) and a ~3% radius error on a 0.17 pt speck is a real, if small, loss,
  so it is left for a decision (follow-up issue).
- **Leading-zero stripping** (`.5`): -0.005%, nothing.
- **Zlib level 9 instead of 6**: -1.3%, 2x the compression time. No.
- **Relative path operators**: PDF has none (`m l c v y h re` only).
- **`v` / `y` curve operators**: need a control point equal to an endpoint; the
  quarter-circle `c` of a speck has neither. The 4-`c` circle is already the
  compact form.
- **`re` for rectangles**: only exact for horizontal-first rectangles, and
  alpine_lake has ~100 of them. Not worth it.
- **Form XObject (or circle template) per speck**: a speck fill is one path,
  painted once, so alpha applies to the union; per-speck `Do` composites each
  speck alone and overlapping specks darken differently under alpha, and anti-
  aliased edges of overlapping specks composite differently even when opaque.
  alpine_lake happens to be opaque, but the exporter would need to prove it per
  fill, and the raster would still not be byte-identical.
- Fewer specks / a leaner polygon representation is a `lib/paintkit.ps` change.
