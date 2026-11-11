# #731 part 1: f32 compositing pipeline (extra 8-bit alpha loss)

Part of #731. The float/16-bit compositing buffer (part 2) is still open.

## What was wrong

Stacking many faint layers stalled short of the colour the maths gives, and
earlier than 8-bit storage alone forces. tiny-skia picks its lowp pipeline
for a plain solid-colour paint. That pipeline divides by 255 as
`(v+255)>>8`, so each blend's rounding is biased rather than to the nearest
level. Since #692 every fill and glyph composites through `Pixmap::fill_rect`
with a u8 coverage mask, so the same loss hit any stack of partly covered
pixels: a low `setalpha`, or an opaque shape thinner than a pixel, whose
coverage acts like alpha.

## Fix

`Gfx::paint()` (fills, glyphs, strokes) and the `shfill` paint set
`Paint::force_hq_pipeline`. The blitter copies the flag
(`tiny-skia/src/pipeline/blitter.rs`), so masked `fill_rect` calls get it
too. The f32 pipeline rounds each stored byte to nearest. Images already
composite in f32 with one rounding (`image.rs` `blend_pixel`, #766). Its
unit test now builds its reference fill the same way, and the tolerance is
1 level instead of 2.

The alpha byte stays quantised to 1/255 (`set_color_rgba8`). At 8-bit
storage a quantised alpha is the better choice: 0.002 rounds up to 1/255
and climbs to 128, whereas an exact 0.002 would stall at about 5, where
`(255 - v) * a` falls below half a level.

## Numbers

White over black, the issue's program (`255·(1−(1−a)ⁿ)` is the float target):

| alpha × layers | float | 8-bit model | before | after |
|---|---|---|---|---|
| 0.01 × 200 | 221 | 213 | 192 | 213 |
| 0.05 × 40 | 222 | 224 | 221 | 224 |
| 0.002 × 1000 | 221 | 128 | 128 | 128 |

The 8-bit model quantises alpha to 1/255 and rounds every blend to nearest.
pscat now matches it on every row, which is the part 1 target. The 0.05 row
is one level further from the float value than before, because lowp's bias
happened to cancel the quantised alpha's overshoot there. The remaining gap
to float, including the 0.002 stall, is what part 2's wider buffer is for.
The same table holds with opaque sub-pixel rectangles (0.01/0.05/0.002 pt
wide) in place of `setalpha`. Pinned by
`gfx::tests::stacked_low_alpha_matches_the_8bit_model` (full and half
coverage).

## Cost

Release build, `--dpi 144`, base and head runs interleaved, median of 5
(3 for lincoln). The machine was shared with other agents' builds, so
±5% is noise:

| file | base | head | delta |
|---|---|---|---|
| gallery/lincoln_charcoal.ps | 32.00 s | 30.27 s | −5.4% |
| gallery/crayon_drawing.ps | 9.02 s | 8.83 s | −2.1% |
| gallery/chalkboard_lecture.ps | 7.56 s | 7.57 s | +0.1% |
| gallery/keepin_it_real.ps | 4.45 s | 4.47 s | +0.6% |
| gallery/time_traveler_memo.ps | 4.32 s | 4.62 s | +6.9% |
| gallery/firefly_census.ps | 1.07 s | 1.05 s | −1.2% |

Opaque, fully covered runs still take tiny-skia's memset fast path, which
never enters a pipeline.

## Why unconditional

The other option applied hq only when the alpha byte was below 255 or the
blend was not Normal. That variant moved 7 of 196 corpus pages instead of
153, but it left the sub-pixel case broken: 200 opaque 0.01 pt rectangles
still stalled at 192. Coverage is the same quantity as alpha in the
compositor, so the fix applies to both.

## Render change

153 of 196 corpus pages (examples, gallery, selftest drivers, site/learn)
move. Most change by 1–3 levels on anti-aliased edges and glyph outlines,
the mean absolute change on changed pixels is about 1.0–1.5, and 85 pages
have a maximum change of 3 levels or less. The largest are the typewriter
pages (`examples/released_pages.ps` page 1: max 37, mean 3.3 on changed
pixels, 8% of pixels over 2 levels), where many overlapping inked glyph
passes stack partial coverage. They come out slightly lighter.
