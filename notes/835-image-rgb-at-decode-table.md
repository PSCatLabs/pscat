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

Measured with pscat only, release build, `--page 983x983 --dpi 300
--headless` (a 4096x4096 device page), default Decode. Instructions retired
from `/usr/bin/time -l`; CPU seconds from `scripts/bench_compare.sh`, best of
5, with the identical-binary control within 0.01 s of the base:

| case | instr base | instr head | CPU base | CPU head |
|---|---|---|---|---|
| RGB 8-bit 4096², file | 8.26 G | 6.81 G | 0.26 | 0.23 |
| RGB 8-bit 4096², procedure | 5.54 G | 4.08 G | 0.16 | 0.13 |
| gray 8-bit 4096², file | 4.78 G | 4.57 G | 0.16 | 0.16 |
| gray 8-bit 4096², procedure | 3.88 G | 3.67 G | 0.13 | 0.11 |
| CMYK 8-bit 4096², file | 10.16 G | 8.03 G | 0.34 | 0.28 |
| CMYK 8-bit 4096², procedure | 6.55 G | 4.41 G | 0.20 | 0.14 |
| RGB 8-bit 4096², file, 0.5 setalpha + Multiply | 8.87 G | 7.42 G | 0.30 | 0.24 |
| RGB 8-bit 1024² magnified 4x, /Interpolate true | 16.24 G | 10.67 G | 0.51 | 0.41 |
| RGB 8-bit 1024² magnified 4x, /Interpolate false | 5.64 G | 3.01 G | 0.15 | 0.08 |
| RGB 8-bit 2x2 stretched over the page | 5.46 G | 2.39 G | 0.15 | 0.06 |
| 5000 8x8 RGB images | 2.02 G | 1.32 G | 0.06 | 0.04 |
| RGB 16-bit 4096², procedure (no table) | 5.94 G | 5.98 G | 0.17 | 0.17 |
| Indexed 8-bit 4096², procedure (no table) | 4.52 G | 4.49 G | 0.14 | 0.15 |
| RGB 16-bit, 15 renders looped | | | 2.55 | 2.55 |

Gray gains little: one component was already cheap. Depths without a table
(12, 16 bits) and Indexed images run the direct code, chosen once per cell.
They came out 2 to 4% slower until `rasterize` was marked `#[inline(never)]`:
inlined into `step` (which the larger `rgb_at` allowed) the per-pixel loop
compiled worse. Out of line they are at parity, and the table cases gain a
little more. File sources stay dominated by reading the
data a byte at a time (a bulk read in `fill` would be the next step; it lives
in `src/file.rs`).

Output is unchanged: unit test `decode_table_matches_the_direct_decode_bit_for_bit`
(depths 1-8, 1-4 components, inverted/clamped/short Decode, short data,
planes, Separation; compared with `to_bits`), the two-binary identity check,
and SVG/PDF byte comparison. Three deliberately broken heads (fast-path
offset +1; table inverted for depths under 8; memo keyed on the row only)
each make image drivers differ, so the corpus reaches every new path. The
memo is PNG-only (SVG and PDF never go through `rasterize`).
