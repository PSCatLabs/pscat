## Four paths #1217 left past the limits (issue #1219, 2026-11-16)

#1188 (`notes/1188-single-operator-limits-remaining.md`) left four paths that
ran past `--max-time` / `--max-memory`. Each was measured on `main` first
(release build, 72 dpi unless stated), then fixed or, for the last, left.

| Item | Before | After |
|---|---|---|
| `clip` of 100,000 page-sized curves, `--max-time 0.2` | 1.9 s at 72 dpi, 7.4 s at 300, blamed on the next operator | stops inside `clip` at 0.23 s / 0.25 s |
| the same, no limit | 1.9 s / 7.4 s | 1.3 s / 5.6 s |
| `shfill`, 4095-leg ramp (1024 stops), 300 dpi letter, `--max-time 0.2` | ran 0.63 s in full, blamed on the next operator | stops inside `shfill` at 0.24 s |
| Type 1, one glyph calling a 16 MB subroutine, 200 glyphs | 4.6 s of CPU | under 0.2 s (500 KB decrypted a glyph) |
| `kernelimage`, 8000 x 8000, every pixel retired, `--max-time 1` | 2.2 GB RSS, stops at 1.4 s | not changed (see below) |

### `clip`

`clip` rasterised through tiny-skia's `Mask::fill_path`, which cannot be
stopped. Its cost is the edges times the sub-scanlines each crosses (the 4x4
supersampler visits every active edge on four sub-scanlines a row): 640M
units for the 100,000-curve path at 72 dpi and 2.6G at 300, both 2.8-2.9 ns a
unit.

- `PsPath::raster_work(height)` estimates that in one pass over the segments:
  4 units a row of each edge (a cubic cut where it turns in y, a subpath
  closed as a fill closes it), clipped to the canvas, and 16 an edge for
  setting it up.
- `Gfx::clip_coverage` is the one place a clip mask is made, for `clip` and
  for `fit_clip` (a clip refitted to another page size keeps its pixels). It
  holds `work / 1024` against the step limit first, so a refused clip leaves
  the clip and the current path as they were, then:
  - under `POLLED_CLIP_WORK` (2^25, about 0.1 s of tiny-skia) it is
    `Mask::fill_path`, unchanged, so the pixels are what they were;
  - at or over it, `coverage::rasterize_limited`, the fill rasteriser, which
    looks at time, heap and steps as it goes. Solid runs are written as 255
    (the rasteriser leaves them to the compositor).
- The threshold was set from data, as #1188 set the stop count. A probe
  build logged the estimate for every `clip` of all 168 gallery, examples and
  driver files: at 72 dpi the largest is 4.8M (`gallery/lincoln_charcoal.ps`),
  at 144 dpi 5.3M, at 300 dpi 6.5M. The threshold is five times that.
- This is a deviation: above the threshold the clip's edge is anti-aliased by
  area coverage, where below it is the 4x4 sampler's. A unit test
  (`the_polled_clip_mask_is_the_supersampled_one_to_within_a_shade`) bounds the
  difference on a curved shape: no pixel differs by more than 48 of 255, and
  the mean is far below that. Nothing in the corpus reaches the threshold, so
  no committed render changes (identity check below).
- The operator charges `work / 1024` steps, as `fill` does, so `steps_used`
  moves for a program that clips under a limit (a rectangle at 72 dpi is 3).

### `shfill`

With no `/BBox` the paint region is the page rectangle, whose edges are
integers: no pixel is partly covered, so painting it in strips of whole rows
gives the same pixels as the one fill. Under the limits the strips (about 4M
units each, a few ms) look at the probe between them, and the work
(`pixels x (16 + stops / 4)`) is held against the step limit before
`prepare_paint`. A `/BBox` is a parallelogram whose edges a strip would cut, so
that case stays one fill, preceded by a look at the limits. SVG and PDF are
recorded after the raster has painted, as before.

The `/BBox` case is therefore still unpolled: at most the 0.6 s a 1024-stop
ramp costs at 300 dpi letter (2.4 s at 600 dpi).

### Type 1

Each operator costs one of the glyph's 100,000 `OP_BUDGET` and reads its own
byte and at most four operand bytes after it (255 and a 32-bit number). The
budget error is raised at the start of the 100,001st operator, whose byte is
at index 500,000 at most, and no byte after it is read. So
`type1::MAX_CODE_BYTES` = 5 x `OP_BUDGET` + 1 plaintext bytes of a charstring
or subroutine are all a glyph can reach, and `type1_glyph` decrypts no more
(plus the `lenIV` lead). The result is exact, with no change to the
interpreter loop. The same cap applies to a `seac` component, which shares the
budget. The memory check before a decryption is made on the capped length.

This is a bound by the operator budget as the issue proposed, without lazy
decryption: a font's own subroutines are small, so nothing is gained by
decrypting less than 500 KB lazily.

### `kernelimage` (not changed)

With `--max-memory` set the retired list is already capped
(`retire_cap`, #1188): 8000 x 8000 all retired under `--max-time 1
--max-memory 200` is 42 MB. With no memory limit there is nothing to cap: it
is 2.2 GB under `--max-time 1`, as an array of the same size would be, and the
clock stops it. No `one_operator` test can be written for it, since no limit
the program has set says to stop. A follow-up could stop a run that has any
limit and retires past a fixed ceiling of pixels (say 16M, 256 MB) through the
existing `Stop::Cancelled` fallback, the interpreter doing the whole image. It
would slow an image that is mostly compiled and retires many pixels, and
changes nothing in the result.

### Tests

`tests/limits.rs`, `one_operator::issue_1219`:

- `a_clip_of_very_many_curves_stops_inside_clip` (`clip`, `eoclip`) and the
  nested variant (a clip in force already) stop with `limitexceeded (time)`
  blamed on the operator;
- `a_step_limit_refuses_a_huge_clip_and_leaves_the_clip_alone`;
- `shfill_painting_stops_inside_shfill` and
  `a_step_limit_refuses_a_shfill_before_it_paints`;
- `type1_subroutines_decrypt_only_the_bytes_a_glyph_can_reach`, a budget
  test (200 glyphs, each with a 16 MB subroutine, finish inside 1 s of CPU).
  Nothing in it stops at a limit: the decrypt had no stop to make, it was
  waste.

Each fails with its fix reverted (clip and shfill by blame and budget, Type 1
by the budget: 4.6 s) and under `PSCAT_TEST_BUDGET_SCALE=0.001`. The
`limits_memory.rs` Type 1 case changed: a 16 MB subroutine is held by a 20 MiB
cap now (it decrypts 500 KB) and refused by a 512 KiB one.
