## chalk: the overlay's radical bar and small strokes stay solid (issue #713, 2026-10-26)

`pscat chalk --no-board` (and the MCP tool's `board: false`) draws chalk on a
transparent page at touch 0.34, light enough that the grain shows as gaps. #688's
perceptual reviewer saw the superscript and the tail of the √ as thin and gappy
there.

### What the measurements said

An ink profile along each stroke's own centreline (ink mass in a 8 px band,
and the share of centreline pixels under half alpha), over 12 seeds at 72 dpi:

- The superscript is not thinner than the same digit at full size (`2^2`:
  effective width 2.4 against 2.1; gap share 0.27 against 0.34). What reads
  as thin is that it is the same stick on a glyph 0.7 the size.
- The weak strokes are the long ones. The radical is one stroke (hook,
  diagonal, bar), and its bar is the faintest part (width 1.8, gap share
  0.39, against 2.4 and 0.26 for a digit). A plain 300 pt line is the same
  horizontal or vertical: 2.07 at the landing, 1.65 in the body, 1.28 at the
  lift. So it is length, not direction (not the surface's grain).
- Cause: `dcchalk`'s profile is in fractions of the stroke. A long stroke
  tires from 30% to 75%, and thins and fades over its last quarter and a
  half, hundreds of points; a touch step of 0.03 is the difference between a
  solid and a gappy mark at 0.34.

### What changed

- `lib/drychalk.ps`: `/Firm` (0..1, default 0, bit-identical: the 23 pages that
  load drychalk, `gallery/chalkboard_lecture.ps` among them, render byte for
  byte). It moves the profile toward absolute lengths. The width taper and
  the touch lift cover at most the last ten widths; the tired-middle dip fades
  as the stroke grows past two dozen widths; the body is pressed up to two
  bands harder on a short stroke and up to 1.2 on a long one. Dots keep their
  own profile. `dctouchat` and `dcpressat` read the landmarks from per-stroke
  globals that both stroke kinds set (the side stroke keeps the old values).
- `src/chalk.rs`: `/Firm 1` only when there is no board. The board's touch of
  0.68 has no gaps to close, so the five board pins stay as they were: that is
  the proof boards look no worse.
- `tests/chalk.rs` pins the overlay (`the_overlay_renders_as_pinned`);
  `tests/drychalk.rs` pins `/Firm` against the default on long strokes.

### Result (12 seeds, `2^2 \sqrt{2}`, 72 dpi)

| | effective width (before -> after) | gap share (before -> after) |
|---|---|---|
| radical bar | 1.83 -> 2.08 | 0.39 -> 0.29 |
| radical overall | 2.42 -> 2.64 | 0.29 -> 0.23 |
| a digit (reference) | 2.15 -> 2.46 | 0.34 -> 0.26 |

### With #778 (exact-area coverage)

#778 changes how a thin fill's coverage is computed. #713 reproduces on its
branch unchanged (bar 2.02 / 0.38 there) and the fix moves it the same way
(2.27 / 0.29): the two compose, and neither makes the other over-solid.

### Not done

The superscript has no rule of its own: drychalk sees a path, not a glyph's
size, and measuring it showed no thin ink to fix. A script-size-aware stick
would need mathkit to tell `/Ink` the level of each atom.

### Re-pinning

No board pin moved (`/Firm` applies only with no board): that is the "boards look no worse" evidence. The one new pin is the overlay's. It is made of sub-pixel specks, so #778 will move it: whichever of the two PRs merges second re-pins `the_overlay_renders_as_pinned`, Windows value included.
