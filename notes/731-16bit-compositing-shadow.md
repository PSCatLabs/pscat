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
- **Three loops.** Translucent source-over runs (solid and masked) blend
  in integers (16-bit values, `1 - alpha` in Q16), which keeps a page-sized
  wash near tiny-skia's speed. Opaque masked runs and Multiply use the
  float loop above, where the tiny-skia match matters.

## Memory

8 bytes a pixel on top of the page's 4, allocated a row at a time when a
fill first composites into the row. Under `--max-memory` the shadow is
used only when a whole page of it takes at most a quarter of the cap
(`Deep::allowed`). The rule depends only on the page size and the cap, so
a render never depends on how much heap is free at the moment; with the
shadow off, fills composite on the 8-bit page as in part 1. At the
untrusted cap (1 GiB, the MCP server's) that is pages up to 32 million
pixels, a 5,600 x 5,600 canvas.

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

All within the noise floor. Two synthetic worst cases show the cost of the
compositing itself: 300 page-sized fills at 0.05 alpha take 0.97 s against
0.75 s (+29%), and 20,000 small discs at 0.3 alpha 0.14 s against 0.10 s.
The first float-only version took 7 s on the wash; the integer loops for
translucent source-over brought it to this.

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
