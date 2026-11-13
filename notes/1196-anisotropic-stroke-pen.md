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
`M` applied to the user-space stroke. The raster hands `L` to tiny-skia's
`stroke_path` as its transform, so tiny-skia dashes and strokes in pen space
and keeps its thin-line coverage. `strokepath` dashes and strokes at
`L`'s resolution scale and maps the outline by `L`. SVG writes
`<path transform="matrix(L)">` inside the clip groups, which stay in device
space. PDF writes `L` as a `cm` inside the stroke's `q … Q`, at full f32
precision, since its entries are near 1.

The old device-space path is kept, unchanged, wherever it is already exact.
That is a similarity CTM: `‖L‖²_F − 2 ≤ 1e-5`, a pen at most 0.3% out of
round, well above the f32 noise of a stack of rotations. It is also kept for
a hairline (`0 setlinewidth` stays one device pixel) and a singular CTM. It
is the fallback for a pen more than 10⁶:1 flat, and for a path whose
pen-space image leaves f32 range, rather than risk a stroke tiny-skia would
drop.

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

**Identity.** Every tracked `gallery/*.ps` and `examples/*.ps` was
compared, base binary against head binary (see the PR for the result).
