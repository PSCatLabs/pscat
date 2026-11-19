# #731 part 2: a 16-bit shadow of the page for fills

Part 1 (`notes/731-hq-compositing-pipeline.md`) brought pscat to the 8-bit
model. An 8-bit page still cannot hold a deep stack of faint layers: once
`(target - current) * alpha` falls under half a level a layer changes
nothing. This part adds the wider buffer.

## Design

`src/deep.rs` keeps a 16-bit premultiplied RGBA copy (`u16` per channel)
of each pixel a fill composites. `Gfx::fill_coverage_limited` (every
`fill`, `eofill`, `rectfill` and glyph) blends each coverage run there,
with the exact `setalpha` rather than its byte, and writes both the 16-bit
value and the 8-bit `pixmap` pixel. The `pixmap` stays the page every
reader sees (PNG, window, WebAssembly, sheets, bands), so nothing
downstream changed.

- **Opaque, fully covered runs** still go to tiny-skia's `fill_rect` (its
  memset fast path), and the shadow only notes the new colour in rows it
  already holds. A page-sized opaque fill costs what it did.
- **Other writers need no hook.** Strokes, images and `shfill` still write
  only the 8-bit page. Each 16-bit pixel is checked against its 8-bit pixel
  before use (`|d - 257 p| <= 129`, the rounding bucket); a pixel another
  writer moved is reloaded from the page. A pixel another writer left in
  the same bucket keeps the 16-bit value, which is within half a level of
  what that writer meant.
- **NaN alpha.** `setalpha` keeps a NaN; it paints nothing, as its byte 0
  did, rather than a NaN blend writing a transparent hole.
- **Erase** (`erasepage`, a showpage's pending erase, a new page device,
  `--transparent`) drops the shadow. A size change drops it too (`fit`).
- **Matching tiny-skia.** At alpha 1 a first blend over a pixel the shadow
  had not seen gives tiny-skia's f32 pipeline's bytes exactly, for both
  blend modes, every coverage and a solid run (`a_first_opaque_blend_matches_tiny_skia`,
  20,000 random cases): the same inputs (the colour byte over 255, the
  page byte times tiny-skia's `1/255`), the same float operations in its
  order (`source_over`/`multiply`, `lerp_u8`, round to nearest even). So
  the edges and glyphs of opaque art render as before; only stacked or
  translucent pixels move.
- **Two loops.** Translucent source-over runs (solid and masked) blend
  in integers (16-bit values, `1 - alpha` in Q16, coverage folded into the
  source), which keeps a page-sized wash near tiny-skia's speed. Opaque
  runs and Multiply use the float loop above, where the tiny-skia match
  matters. An opaque source-over pixel at full coverage inside a masked
  run (the inside of a clipped fill) is written as the colour directly.
  Multiply in integers (three products over 65535, in `u64`) measured
  slower than the float loop, so it stays in floats.

## Memory

8 bytes a pixel on top of the page's 4, allocated a row at a time when a
fill first composites into the row. Under `--max-memory` the shadow is
used only when a whole page of it takes at most a quarter of the cap
(`Deep::allowed`). The rule depends only on the page size and the cap, so
a render never depends on how much heap is free at the moment; with the
shadow off, fills composite on the 8-bit page as in part 1. At the
untrusted cap (1 GiB, the MCP server's) that is pages up to about 33.5
million pixels (a 5,790 x 5,790 canvas); a bigger page under that cap
renders as part 1 did, so its bytes differ from an uncapped CLI render.
Under `--bands` each band keeps a shadow only for its own rows (the rest
are discarded at the stitch), so the bands' shadows add up to one page.

## Numbers

The issue's program, white over black (`255(1-(1-a)^n)` is the float value):

| alpha x layers | float | part 1 (8-bit) | now |
|---|---|---|---|
| 0.01 x 200 | 221 | 213 | 221 |
| 0.05 x 40 | 222 | 224 | 222 |
| 0.002 x 1000 | 221 | 128 | 221 |

Pinned by `gfx::tests::stacked_low_alpha_reaches_the_float_value` (full and
half coverage), `stacked_sub_pixel_shapes_reach_the_float_value` (200
opaque 0.05 px rectangles) and `the_shadow_follows_strokes_and_new_pages`.

## Cost

**Speed.** CPU seconds (`scripts/bench_compare.sh`, min of 3, `--dpi 144`,
base = `feature/0.7.1` at b131039). The control is the base binary run again:

| file | base | head | control |
|---|---|---|---|
| gallery/alpine_lake.ps (watercolor washes) | 21.42 | 21.49 | 20.34 |
| gallery/first_rain.ps | 0.20 | 0.19 | 0.19 |
| gallery/chalkboard_lecture.ps | 4.81 | 4.81 | 4.82 |
| gallery/time_traveler_memo.ps (typed text) | 3.33 | 3.24 | 3.23 |
| gallery/crayon_drawing.ps | 6.34 | 6.36 | 6.29 |
| gallery/firefly_census.ps | 0.74 | 0.74 | 0.74 |

All within the noise floor. Synthetic cases at `--dpi 144`, min of 3 CPU
seconds, base / head / control:

| program | base | head | control |
|---|---|---|---|
| 300 page-sized fills at 0.05 alpha | 0.76 | 0.97 | 0.75 |
| 300 page-sized Multiply fills at 0.05 alpha | 0.99 | 1.61 | 1.00 |
| 300 opaque page fills inside a circular clip | 0.79 | 0.40 | 0.82 |
| 20,000 small discs at 0.3 alpha | 0.11 | 0.14 | 0.11 |

The translucent wash and the Multiply wash pay for the extra 16-bit
read and write on every pixel. A clipped opaque fill got faster, because
its inside is written as the colour rather than blended. The first,
float-only version took 7 s on the wash; the integer loop brought it to
this.

**Render change.** All 232 pages of examples, gallery, selftest drivers and
site/learn, base binary against head (`scripts/identity_check.sh
--base-bin/--head-bin --files`): 136 move, by at most 4 levels; 133 of
them by at most 3, and the mean change on changed pixels is about 1.0
level. The changed pixels are translucent ones (the alpha is now exact,
not a byte) and edges that a second fill blends over (a shared edge, an
overlapping glyph), now blended from the 16-bit value. The largest are
paintkit's wet demos (`paintkit_wet_pull_demo`: max 4, 27 pixels over 2
levels). Nothing is visible at a glance; a deep stack of faint fills, as
in the issue's splat clouds, is where the change shows.

## Not done

- Strokes, images and `shfill` still blend at 8 bits. A deep stack of
  faint strokes (hatching, glows drawn as strokes) still stalls; routing
  them through the shadow needs their coverage as a mask first.
- The page is still 8-bit on output; a 16-bit PNG is a separate question.
