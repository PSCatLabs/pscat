# 692: exact-area coverage for fills

Thin filled shapes used to vanish or snap to one gray. A rule thinner than a
device pixel now keeps a coverage proportional to its area, so hatching,
engraving and dry-media textures fade smoothly as a render shrinks.

## Root cause

pscat handed every fill to tiny-skia 0.12's `fill_path` with anti-aliasing
on. That AA is `scan::path_aa`, Skia's old supersampler with
`SUPERSAMPLE_SHIFT = 2`: each pixel is sampled on four sub-scanlines (and in
quarter-pixel steps across). A rule thinner than a quarter pixel can miss
every sub-scanline and vanish. Anything from a quarter to half a pixel hits
exactly one, so a whole range of widths comes out as the same gray. That
matches the issue's table: 0.04-0.11 px gives 255, 0.22-0.38 px all give 175.

It isn't a flag, path snapping or the fill rule. Strokes were fine because
tiny-skia draws a sub-pixel-wide stroke as a hairline whose alpha is scaled
by the width (`stroke_path`'s thin-stroke case). pscat has no non-AA raster
mode: no `anti_alias = false` anywhere in `src/`. `--halftone` screens the
finished raster afterwards (`src/halftone.rs`), so it stays bilevel and now
starts from truer grays.

## The fix

`src/coverage.rs` is an exact-area rasteriser of the kind font renderers use
(font-rs's line accumulation; FreeType's "smooth" rasteriser applies its fill
rule to the same quantity). Each edge adds its signed, winding-weighted area
to an accumulation buffer, and a running sum along each row gives each
pixel's coverage. Nonzero uses `min(|v|, 1)` and even-odd folds `v` modulo 2.
It is exact wherever only one region's edges cross a pixel. Where edges of
different windings share a pixel it makes the usual small approximation.

- **Sparse.** Only the cells an edge touched are read back and reset. A row's
  coverage is constant between them, so a large interior becomes one *solid*
  span, and identical spans on consecutive rows merge into one rectangle.
  The work depends on the number of edges and partly covered pixels, not on
  area. Curves are flattened to 0.05 px, with at most 1024 chords each.
- **Compositing is unchanged.** `Gfx::fill_coverage` writes the coverage into
  a cached page-sized `tiny_skia::Mask` (times the clip mask, if any). It then
  calls `fill_rect` per run with AA off: solid runs go without the mask, which
  is tiny-skia's opaque fast path, and the rest go with it. Colour, `setalpha`
  and blend modes still go through tiny-skia's pipeline.
- **Small paths.** A path whose bounds are at most 4096 px is composited as
  one masked rectangle, because one call beats several.
- **Scope.** The change covers `fill`, `eofill`, `rectfill` and glyph fills
  (`fill_path_direct`). Strokes, `clip`/`eoclip` mask building and `shfill`
  still use tiny-skia's rasteriser.

### Repro (issue table, `--page 384x384 --dpi 9`)

| H (pt) | px | area says | before | after |
|---|---|---|---|---|
| 0.3 / 0.6 / 0.9 | 0.04-0.11 | 242 / 230 / 217 | 255 / 255 / 255 | 242 / 230 / 217 |
| 1.2 | 0.15 | 204 | 175 | 206 |
| 1.8 / 2.4 / 3.0 | 0.22-0.38 | 179 / 153 / 128 | 175 / 175 / 175 | 183 / 161 / 139 |
| 4.5 | 0.56 | 64 | 106 | 92 |

From 2.4 pt up, two rules share some pixel rows. Each `rectfill` composites
separately (`1-(1-a)(1-b)`, not `a+b`), so the "area says" column is only a
guide there. tiny-skia's analytic `fill_rect`, measured on the same rows,
gives 161 / 140 / 92, matching the after column.

## Scope across the corpus

Two-binary `identity_check.sh` over every tracked `examples/`, `gallery/` and
`selftest/drivers/` file (154 pages), with the issue's ramp as the control:
136 pages differ and 18 are identical. The 18 identical pages are all
stroke-only pieces: cathedral_rose, hundred_lines, silk_waves, golden_spiral,
handwriting, koch_snowflake, and the artkit, mathkit and strokeglyphs drivers.

Percentage of pixels that moved by more than 16 levels:

- under 0.1% on 65 pages;
- 0.1-1% on 61;
- 1-3.4% on 10: lincoln_charcoal, paintkit trowel/broad/fan/wet-under
  demos, the drycrayon specimen and motion demo, crayon_drawing, the font
  catalog's page 2 and paintkit driver page 10.

The changes sit at edges: charcoal and crayon grain, thin bristle and trowel
strips, and inline display faces. Monoton's neon lines now read as a lighter,
even gray instead of dark. Mean luminance moves by at most 1.5 levels per page.

### Timing (guard only)

These are pscat-only wall times with the final binary, on a busy machine
(other agents were rendering), so expect a few seconds of noise.

- Gallery pages, base vs head: lincoln_charcoal had user time 31.8 / 34.7 s
  against 30.9 / 33.4 s over two interleaved runs. crayon_drawing took 9.5
  vs 10.3 s, and alpine_lake 38 vs 31 s (noise).
- Stress files at 300 dpi:
  - 200 page-sized fills: 0.35 vs 0.45 s;
  - 20,000 small discs: 1.44 vs 1.46 s;
  - 200 page-sized fills under a `rectclip`: 10.4 vs 9.5 s. A clip disables
    solid runs, so every covered pixel takes the masked path, but tiny-skia
    was masking those pixels already.
- A first, dense version of the rasteriser took 5.3 s on the unclipped
  page-fill file, against 0.2 s for base. The sparse spans, the solid runs
  and a linear row merge (a hatch row can hold thousands of spans) are what
  fixed that.

## Tests and libraries

- `src/coverage.rs` unit tests cover thin rules, slanted slivers, both fill
  rules on same- and opposite-wound overlaps, edges off the canvas, a banded
  disc's area, run decomposition and clip scaling.
- `tests/render.rs` checks the issue's ramp end to end, with a clip, and
  even-odd against nonzero.
- Re-pinned hashes: the chalk boards, the MCP chalkboard board, and the
  paintkit unset-key, fan, oil and wet renders. The chalk cases board no
  longer carries a separate Windows pin, since `src/fmath` (#738) gives every
  platform the same math.
- `colormix_grades_between_bands` now allows a few levels at its ends.
  Trowel strips abut at fractional positions, and an exact-area seam lets a
  few percent of the page through; the old renderer had such a seam too, at
  another row.
- The pkdab "same dabs" test now compares ink against clear pixels. Edge
  pixels a level off white could flip when a shadow colour changed.
- `lib/drychalk.ps`: the tray's first floor strip overlapped the board by
  0.01 pt to hide a seam. With exact coverage that tinted the board's last
  row, so the first strip now stops at the board's edge.

## Found for #731 (not fixed here)

The extra loss beyond 8-bit storage is tiny-skia's lowp pipeline. Its
`div255` is `(v + 255) >> 8` (`pipeline/lowp.rs`), which biases every blend
toward the destination. With the alpha byte `to_u8` rounds to (3 for 0.01, 1
for 0.002), source-over white over black stalls where
`s + ((d*(255-a) + 255) >> 8) == d`:

| alpha x layers | lowp model | exact 8-bit model | pscat 0.6.2 |
|---|---|---|---|
| 0.05 x 40 | 221 | 224 | 221 |
| 0.01 x 200 | 192 | 213 | 192 |
| 0.002 x 1000 | 128 | 128 | 128 |

The lowp model reproduces pscat exactly. `Paint::force_hq_pipeline = true`
selects tiny-skia's f32 pipeline, which rounds once per blend, and is the
likely one-line fix for #731's first part. Masked fills (this PR) still take
the lowp path when the paint allows it.

## Follow-ups

- `clip`/`eoclip` masks still come from tiny-skia's supersampler, so a clip
  thinner than a pixel stays quantised.
- Thick strokes are filled by tiny-skia's stroker and keep supersampled
  edges. Only sub-pixel strokes take the hairline path.
- The committed gallery stills (`gallery/renders/`) predate this change.
  Regenerate them with the next stills refresh.
