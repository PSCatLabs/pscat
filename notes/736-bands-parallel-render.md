## `pscat --bands N`: parallel banded PNG renders (issue #736, 2026-10-25)

Reported by fractal_clouds, whose `scripts/render_bands.sh` started one pscat
per horizontal band and stitched the PNGs. `--bands N` does that in one process.

**Threads, not processes.** Each band is a separate, complete run of the
program in its own `Interp` on its own thread (`src/bands.rs`, a module of the
`pscat` binary). Nothing PostScript-visible is shared between them. The name
interner (`name.rs`) and the array-teardown placeholder (`object.rs`) are
`thread_local!`. The process-wide state (font catalog `RwLock`, embedded faces,
resource roots and font-pack tables in `OnceLock`s) is `Sync`. RNG and noise
state live in the `Interp`. `Interp` is `!Send` (it holds `Rc`s), so each band
builds its own on its thread and hands back plain `Pixmap`s and error
`String`s. Band threads get an 8 MiB stack, the size the main thread runs a
plain render on (a 10k-deep array and 100k open braces still end in a
PostScript error, not a crash).

**Interface.**
- `userdict` gets `/BandIndex` (0 = top), `/BandCount` and
  `/BandBBox [llx lly urx ury]` (the band's area in default user space, points)
  before the program runs.
- Band `i` of `N` on a canvas `H` px tall keeps device rows
  `floor(i*H/N) .. floor((i+1)*H/N)`. pscat copies those whole rows from each
  band's page into one page.
- The only device-level saving for an unaware program is image rasterization
  (`image.rs`'s per-pixel loop clamps to the band's rows, `Gfx::set_band_rows`).
  Every device pixel there is computed on its own, so the band's rows come out
  exactly as in a full render.
- There is no PostScript clip, and no tiny-skia mask either. A mask would not
  speed anything up (tiny-skia still rasterizes the whole path), and a clip
  would change what `clippath`/`pathbbox`-sized programs see. Fills, strokes,
  glyphs and shadings run in full in every band. That is why the stitch is
  byte-identical, anti-aliased seams included.
- The canvas size is fixed for the run (pscat has no `setpagedevice`), so every
  band's pages are the same size as the stitched page.
- Multi-page: every page is banded. Bands that emit different page counts are
  an error. PNG only: `--svg`, `--pdf`, `--lint`, sweeps, `--progress` and
  `--allow-write` (every band would write the file) are refused.
- Only the top band's stdout reaches the terminal; the rest is captured in a
  bounded buffer and dropped. An error is reported once, from the first band
  that failed, and the stitched partial page is still written (exit 1).

**Limits.** `--max-steps` and `--max-time` apply per band (an unaware program
takes the same steps in each band as in a plain run, so the same limit fits).
`--max-memory` reads the process heap counter (`memtrack`), so it caps all
bands together. Two gates (a mutex and condvar each, not `Barrier`s) hold the
bands:
- the first until all N canvases exist, so every band's heap base includes them;
- the second until every band has set its base, so no program's allocations
  land under a later band's base (Codex round 2).

A drop guard aborts both gates however a band ends, so a failure or panic never
leaves the others waiting. The limits go on after the band names are defined,
so a tight `--max-steps` bounds the program alone (Codex round 1).

`--read-root`/`--confine-reads` (#763, merged while this was open) are set on
every band's interpreter, as on a plain render.

**Evidence** (10-core Apple M-series laptop: 4 performance + 6 efficiency
cores; wall time, median of 5 interleaved rounds after a warm-up round, pscat
only). The piece is a band-aware 1600x1200 fbm2 `colorimage` procedure
(8 octaves) at 280x210 pt, 150 dpi:

| N | wall | speedup vs plain |
|---|------|------------------|
| plain | 1.81 s | 1.00x |
| 1 | 1.79 s | 1.01x |
| 2 | 0.94 s | 1.93x |
| 4 | 0.66 s | 2.74x |
| 8 | 0.46 s | 3.90x |

Past 4 bands the extra threads land on efficiency cores, so the curve
flattens. The same piece with its band check disabled (unaware) gets 1.01x at
N=1, 0.99x at 2, 0.79x at 4 and 0.61x at 8: no speedup, plus contention. Every
banded PNG was byte-identical to the plain one.

**Tests.** `tests/bands.rs` checks byte identity against the plain render for:
- a rotated `colorimage` procedure;
- the band-aware fbm2 piece;
- strokes, curves, alpha, a dash, text, a clip and a gradient crossing every
  seam (also with `--transparent` and `--halftone`);
- `examples/fractals.ps` (artkit).

The band counts are 2, 3, 7 and 8 at 100 dpi, so the seams fall off whole
points. A stripe control (each band paints its own grey) checks where the
rows land, since an unaware program would pass any compositor. Making the
image clamp one row short fails the two image tests.

**Not done (follow-ups).** Culling fills, strokes and glyphs whose device bbox
misses the band would help unaware vector-heavy pieces. It needs a truly
conservative stroke bound (miter, caps, dashes, the anisotropic width
approximation), and #692 was changing fill coverage at the time. `pscat-mcp`
does not expose bands.
