## Byte-aligned fast path in image `sample()` (issue #793, 2026-10-26)

`ImageCtx::sample` (src/image.rs) read every sample bit by bit, at every
depth. For 8- and 16-bit data (the common cases) it now reads one or two whole
bytes directly. Bytes past the end of the buffer still read as 0, so a short
data source behaves exactly as before; 1, 2, 4 and 12-bit data keep the
general loop. No behaviour change: PNG, SVG and PDF output are byte-identical
(two-binary `identity_check.sh` over the artkit corpus plus the image drivers,
SVG/PDF cmp over the six image drivers, and a unit test comparing the fast path
with the bitwise definition, short buffers included).

Measured with pscat only, release build, CPU seconds, 4096x4096 image painted
at 300 dpi (best of 5, base binary vs head, identical-binary control within
0.01 s):

| source | base | head |
|---|---|---|
| RGB 8-bit, file | 1.01 | 0.90 |
| RGB 16-bit, file | 1.20 | 1.02 |
| RGB 8-bit, procedure (one row per call) | 0.22 | 0.11 |

The file cases are dominated by reading and filtering 50 to 100 MB, so the gain
there is about 11 to 15 percent; with the data already cheap to supply (the
procedure case) `sample()` was half the render and the render is twice as fast.
Deferred: the remaining per-pixel cost is `decoded()`'s float work and the
nearest-cell lookup, not `sample()`.
