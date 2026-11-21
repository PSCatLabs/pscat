## A ShadingType 1 Domain pair equal in f32 paints nothing (issue #1333, 2026-11-19)

**The bug.** A function-based shading whose `/Domain` pair differs in f64 but
is equal once cast to f32 (`/Domain [1000000 1000000.001 0 1]`, with a
`/Matrix` that scales it to a visible strip) painted a strip in the PNG, and
the SVG embedded it. The PDF wrote `/Domain [1000000 1000000 0 1]`, a
zero-width Domain, and a reader painted nothing. The three outputs disagreed.

**Which one is right** (gs 10.07.1, `png16m` at 72 dpi, from the `.ps` and
from the PDF pscat wrote):

- `/Domain [1000000 1000000.001 0 1]`, and the same on y
  (`/Domain [0 1 1000000 1000000.001]`): gs paints nothing and raises no
  error, from the PostScript and from the PDF.
- `/Domain [1000000 1000000.125 0 1]` (0.125 is two f32 steps at 1e6, so
  the pair stays distinct): gs paints the strip.

So gs reads the pair as f32 and treats an equal pair as zero-width, which
paints nothing. The PNG was the outlier. This is the rule the axial and
radial shadings already follow when an axis collapses in f32 (#1041).

**The fix.** `parse_function_shading` makes such a pair exactly zero-width
(`x1 = x0`, after the check that the Domain lies in the function's own, so
that still sees the numbers as written). `shfill_function_limited` already
returns before painting or recording for an exactly zero-width Domain, so the
PNG, the SVG (which embeds the raster's layer) and the PDF all carry nothing,
as gs paints nothing. No change in `src/pdf.rs` was needed.

**Not done.** A pair that stays distinct in f32 still loses precision in the
PDF: its Domain and Matrix are written as f32, the raster uses f64. The
translate in the repro came back as `-100000000000` for `-99999999950`, so
at a base of 1e6 the PDF is shifted from the PNG. gs itself evaluates the
distinct-pair control coarsely there (it paints three flat bands where the
raster paints the gradient), so no gs comparison is pinned for it. A fix
would normalise the Domain to `[0 1]` and fold the shift into `/Matrix`.
