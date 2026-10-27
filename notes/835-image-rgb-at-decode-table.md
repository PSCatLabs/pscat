## Decode table and 8-bit fast path in image `rgb_at` (issue #835, 2026-10-26)

Follows #793 (byte-aligned `sample()`). Profiled first (release build, macOS
`sample` on looped renders): for a 4096x4096 CMYK file source, reading the
file byte by byte (`PsFile::read_byte` under `ImageCtx::fill`) was about 46%
of the render, `sample` + `rgb_at` about 32% and the per-pixel loop in
`rasterize` the rest; for a magnified `/Interpolate true` image, `sample` +
`rgb_at` were about 81%. A 2x2 image stretched over a 4096x4096 page cost as
much as a 4096x4096 one: the per-pixel cost follows the device pixels, not
the samples.

What changed (src/image.rs only):

- **Decode table.** For a colour image of 8 bits or fewer per sample (not a
  mask, not Indexed) `step()` builds, once the data is in, a table of every
  sample value's decoded component (at most 4 x 256 floats). `decoded()` reads
  it. Each entry comes from `decode_value`, the very expression `decoded()`
  used to evaluate per pixel, so the floats are bit-identical. A component
  past `ncomp` (the CMYK arm reads four) still takes the direct expression,
  as before.
- **8-bit fast path in `rgb_at`.** For one interleaved source with 1, 3 or 4
  components it computes the cell's byte offset once and looks up each
  component; a cell that runs past the end of short data takes the general
  path, where missing bytes read 0.
- **Last-cell memo** in `rasterize`'s nearest-sample branch, only when the
  image is magnified on some axis (each cell then covers a run of device
  pixels, and `rgb_at` is pure). At one sample per pixel or fewer it would
  never hit, so it is off there.

Instructions retired (`/usr/bin/time -l`, pscat only, `--page 983x983 --dpi
300 --headless`, a 4096x4096 device page; 8-bit, default Decode):

| case | base | head |
|---|---|---|
| RGB 4096², file | 8.29 G | 6.90 G |
| RGB 4096², procedure | 5.54 G | 4.17 G |
| gray 4096², file | 4.79 G | 4.65 G |
| gray 4096², procedure | 3.88 G | 3.75 G |
| CMYK 4096², file | 10.17 G | 8.12 G |
| CMYK 4096², procedure | 6.53 G | 4.51 G |
| RGB 4096², file, 0.5 setalpha + Multiply | 8.89 G | 7.51 G |
| RGB 1024² magnified 4x, /Interpolate true | 16.25 G | 10.78 G |
| RGB 1024² magnified 4x, /Interpolate false | 5.64 G | 3.13 G |
| RGB 2x2 stretched over the page | 5.46 G | 2.50 G |
| 5000 8x8 RGB images | 2.01 G | 1.36 G |
| RGB 16-bit 4096², procedure (no table) | 5.89 G | 6.01 G |
| Indexed 8-bit 4096², procedure (no table) | 4.51 G | 4.57 G |

CPU seconds (`scripts/bench_compare.sh`) are in the PR. Gray gains little:
one component was already cheap. Depths without a table (12, 16 bits) and
Indexed images run the direct code, chosen once per cell; they come out about
1 to 2% slower (16-bit, 15 renders looped: 2.58 s against 2.52 s, noise floor
0.01 s), which turning the memo off does not change, so it is code layout
around the larger `rgb_at`, not extra work. File sources stay dominated by reading the
data a byte at a time (a bulk read in `fill` would be the next step; it lives
in `src/file.rs`).

Output is unchanged: unit test `decode_table_matches_the_direct_decode_bit_for_bit`
(depths 1-8, 1-4 components, inverted/clamped/short Decode, short data,
planes, Separation; compared with `to_bits`), the two-binary identity check,
and SVG/PDF byte comparison. A deliberately broken head (fast-path offset +1)
makes the image drivers differ, so the corpus reaches the new path.
