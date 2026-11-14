## Anisotropic CTMs stroke with an elliptical pen (issue #1196, 2026-11-12)

`rectstroke` with a matrix operand drew every side at the same width, and so
did plain `stroke` under a non-uniform `concat`. The PLRM strokes in user
space with a round pen, so the CTM turns that pen into an ellipse. The
`rectstroke` operator already did what the PLRM says (build the path,
`concat` the matrix, `stroke`, `grestore`). The bug was in the shared
`Gfx::stroke`, which stroked the device path with one width,
`w·√|det CTM|`. That was a documented approximation (gfx.rs module doc,
HANDOFF "Deliberate deviations", ARCHITECTURE). `[2 0 0 0.5 0 0]` has
det 1, so every side came out `w` wide.

Measured on main 70920f7: a 3-wide pen, 100×100 at 72 dpi, with the ink
summed on the middle row and column. gs ran with `false setstrokeadjust`
and `-dGraphicsAlphaBits=4`:

| program | vertical sides | horizontal sides |
|---|---|---|
| gs, `20 20 60 60 [2 0 0 0.5 0 0] rectstroke` | 6.0 | 1.47 |
| gs, `[2 0 0 0.5 0 0] concat` + the same rectangle + `stroke` | 6.0 | 1.47 |
| pscat before, either program | 3.0 | 3.0 |
| pscat after, either program | 6.0 | 1.5 |

Plain `stroke` had the same fault, so the fix went into the shared cause.

**Fix (`Gfx::stroke_space`).** The CTM's linear part `M` is split as `s·L`.
`s = √|det M|` is the factor the width and dashes were already scaled by,
and `det L = ±1`. A user path `U` reaches the device as `M·U + t`, so
mapping the device path by `L⁻¹` gives `s·U` plus a constant: "pen space".
Stroking there with a round pen of `s·w`, dashes of `s·d` and the miter
limit unchanged (it is a ratio), then mapping the outline by `L`, is exactly
`M` applied to the user-space stroke. The raster dashes and strokes in pen
space at `L`'s resolution scale and fills the outline mapped by `L`, which is
what tiny-skia's `stroke_path` does for any pen it does not treat as a
hairline. `strokepath` makes the same outline. SVG writes
`<path transform="matrix(L)">` inside the clip groups, which stay in device
space; its `stroke-width` and dash array are pen units, which the transform
scales (checked by rendering the SVG with Quick Look: 8 and 2 px sides, as in
the raster). PDF writes `L` as a `cm` inside the stroke's `q … Q`, at full
f32 precision, and its dash array is in pen units too (gs reads it back to
the raster's dashes).

**Codex round 1** found four gaps, each now fixed and tested with its case:

- *The 0.1 px width floor* applied in pen space, where `L` magnified it
  (`[100 0 0 0.01] concat 0.01 setlinewidth` drew a 10 px line). In pen
  space it is now `0.1/σ`, where `σ` is `L`'s larger singular value, so the
  pen's widest device extent is at least 0.1 px. `strokepath pathbbox`
  gives ±0.005, as gs does.
- *A pen under a pixel every way.* tiny-skia draws a pen like that as one
  device hairline of averaged coverage, losing the anisotropy: a 0.4 pen
  under `[2 0 0 0.5]` read 0.5 px both ways. Filling the outline instead
  is no good either, because 4×4 supersampling can miss a 0.2 px stripe
  entirely. `Gfx::pen_hairlines` cuts the dashes in pen space, maps the
  path to device space and flattens it. It then groups the segments by
  their own device width, `p·|Lᵀn|` across unit normal `n` (floored at
  0.1, in 1/256 px steps), and draws each group as tiny-skia's modulated
  hairline, the same thing a thin line under a similarity CTM gets. Ink is
  then 0.8 and 0.2 px wherever the line falls on the grid.
  - This only applies when tiny-skia's own hairline test would fire.
  - A pen over a pixel one way and under it the other is filled. Its thin
    direction follows fill sampling, as `strokepath fill` does.
- *Pen-space coordinates in PDF and SVG* were rounded before the map
  stretched them (2 and 3 decimals). Under `[900 0 0 0.0011111111]`, a line
  at x = 103.4 landed at 99. They now get `⌈log10 σ⌉` extra decimals
  (`PenSpace::decimals`, at most 6 more).
- *The round-pen shortcut* used a fixed 0.3% slack, so a 4000-unit pen
  under a 0.15% stretch was 3 px off per edge. The shortcut is now bounded
  by error instead. The round device pen is kept while each pen edge is
  within 1/16 px of the true one: `(σ - 1)·s·w/2 ≤ 1/16`. With a dash set,
  each dash boundary must also be within 1/16 px:
  `(σ - 1)·(path length) ≤ 1/16`.

**Codex round 2** found four more, also fixed and tested with their cases:

- *Hairline caps.* The sub-pixel path drew each segment as its own
  hairline, so round or square caps landed on every segment:
  8000 collinear tiny linetos read 310 px of ink against gs's 31.5.
  - Runs of one device width now stay one connected polyline.
  - Collinear short steps are merged, within 1/32 px (`simplify_polyline`).
    tiny-skia's hairline under-covers a run of 0.02 px steps under any CTM,
    so this also fixes the conformal-looking ink deficit.
  - Every run is drawn butt-capped. Caps go only at the true ends of open
    subpaths, by extending the end points as far as tiny-skia's hairline
    cap would (π/8 or 1/2 px).
  - Closed subpaths get no caps.
- *Zero widths in PDF/SVG.* Pen-space widths and dash arrays now get the
  path's extra decimals, and a nonzero one is never written as `0`.
- *`moveto closepath` dot.* A zero-length subpath is a dot under round or
  square caps, in the raster, SVG and PDF alike, as in gs. Under butt caps
  it draws nothing.
- *Miter tips.* The round-pen error bound now scales by how far the outline
  reaches from the path: the miter limit under miter joins, √2 under square
  caps. Under `[1.01 0 0 .9900990099]`, a 13.45:1 miter tip now sits at the
  user-space point the PLRM gives (x = 167.03, the same as untransformed).
  gs puts it at 167.27 even untransformed.

The device-space path is also kept for a hairline (`0 setlinewidth` stays
one device pixel), for a singular CTM, for a pen more than 10⁶:1 flat
(`‖L‖² > 10⁶`), and for a path whose pen-space image leaves f32 range. Those
fall back rather than risk a stroke tiny-skia would drop.

Timing, user CPU for 20k dashed strokes on one page, base / head:
- 1.5-wide pen under `[2 0 0 0.5]`: 0.15 / 0.15 s
- 0.3-wide pen, which takes the sub-pixel path: 0.06 / 0.11 s (after round 2)
- the same strokes under a uniform scale: 0.13 / 0.12 s

**Tests.** `tests/stroke_anisotropic.rs` covers:
- analytic side widths, with no gs needed, for rectstroke's matrix and
  stroke under concat (non-uniform, rotated and skewed), plus hairlines and
  similarity CTMs;
- the same programs measured against gs, plus dashes and round caps and
  joins;
- strokepath bboxes against gs;
- the SVG transform and the PDF `cm`, with the PDF rasterised back by gs.

gs's 4-bit coverage reads a slanted edge up to ~0.4 px wide. A 5.12 px
skewed side that both `strokepath` outlines put at x = 37.44 to 42.56 is
5.47 px of gs ink. pscat reads 5.0, because tiny-skia snaps edges to a
quarter pixel. So the gs raster comparison allows 0.6 px, and the analytic
table carries the exact widths. `tests/pathbbox.rs` now runs `strokepath`
under its five anisotropic CTMs too. A stroked curve's outline there is
compared flattened, because pscat's curve outline and gs's line outline have
control-point hulls that the anisotropic map pulls apart. The flattened
bboxes agree to 0.4.

**Identity.** Two-binary check over every tracked `gallery/*.ps`,
`examples/*.ps` and `selftest/drivers/*.ps` (168 files, 207 pages): 205
identical, 4 skipped (the `hybrid/compare_*` files fail the same way in both
trees), 2 changed. One is `selftest/drivers/arcto.ps`, whose panel 3
strokes a rounded rectangle under `1.6 0.7 scale`; its sides are now 1.6
and 0.7 of the line width, as gs draws them. The other is
`gallery/released_memo.ps`: one handwritten glyph (the "w" of the
initials) changes at its edges, at most 19/255 on 0.01% of the page.
`lib/handscript.ps` and `lib/hangul.ps` give each glyph a small shear,
`[1 0 k 1 0 j] concat` with |k| up to 0.08. Before round 1 that changed nine
handwriting pages (up to 0.97% of a page's pixels). With the 1/16 px bound,
only that glyph's pen is far enough out of round to take pen space. The
crops are in `docs/dev/review-log/pending/1196-anisotropic-stroke-pen/`.
The committed `gallery/renders/` stills were not re-rendered.
