## chalk: ghost writing keeps out of the formula's box (issue #698, 2026-10-25)

`pscat chalk`'s board dressing draws two or three faint ghost formulas, the
eraser's leftovers of an earlier lecture. They were placed in `src/chalk.rs`
from the seed alone, in bands along the top and bottom of the board, so they
did not know where the writing was. On a wide formula they collided on every
seed (#555's cold reviewer read a ghost beside `f(x)` as a superscript; #699's
agent turned `board.ghosting` off for its review images). Measured before the
fix (below), ghost ink sat inside the formula's box on half the small boards.

### What changed

- `ghost_placement` (was `ghost_paths`) still draws the dice in Rust (which
  formulas, nominal sizes, a `u`/`v` pair and an order of regions per ghost)
  but no longer places anything. The writing's extent exists only after the
  driver's measuring pass, so the placing is PostScript: `GHOST_PLACER`, in
  `ckdict` just before `dcboard`.
- The writing's box is the bbox of the path the writing is inked along
  (`mkalign` with no `/Ink`, then `pathbbox`): the strokes actually drawn, tilt
  included, a little inside the layout's line boxes (`ckx0..cky1`). Writing with
  no ink (`\quad`, `~`) falls back to the line boxes. It is inflated by 0.6 of
  the writing size above and below and 0.9 beside it (round 2 of the
  perceptual review: a ghost just past the end of a row read as the row going on).
- The free board around it is four regions (top, bottom, left, right; top and
  bottom may hang 6% off the sides, as erased writing does; nothing goes onto
  the ledge). Each ghost tries its regions in its own order (top and bottom
  first, in turns, as before), at its size and then at 0.82, 0.66 and 0.52 of
  it, and takes the first fit that no earlier ghost occupies. Its box is its
  real path's (`pathbbox`, tilt included, `/Seed` makes it position-free),
  padded for the chalk's width and dust. A ghost that fits nowhere is dropped:
  a board the formula fills has fewer ghosts, and may have none. The swipes,
  wipes and smudges still make it look used.
- The placements are plain numbers (`ckgplaced`) computed before the board;
  `/Ghost { ckghostdraw }` only calls `mkpath` at them, so none of the
  measuring happens inside `dcghost`'s `stopped` and stack guards.
- `ckdict` is now 80 entries (was 40) for the placer's names.

### Checks

- `tests/chalk.rs::ghost_writing_keeps_clear_of_the_formula`: no render. Runs
  the driver up to the board and compares every placed ghost's real path box
  with the writing's path box, over five formulas (`x = 1`, `F = ma`, an
  integral, the quadratic, a small-board one) x 12 seeds on three board sizes;
  needs a gap of 0.6 of the writing size.
- A pixel sweep: the ghost footprint (board render minus the same board with
  `ghosting: 0`, formula stripped, swipes/wipes/smudges/haze off) against the
  formula's own bright pixels (formula board minus stripped board), 8 formulas
  (`f(x)`, `x = 1`, `F = ma`, an integral, the quadratic, `e^{i pi}`, a sum, a
  2-row align) x 20 seeds x 3 boards. The gap is the Chebyshev distance in px
  at 72 dpi. Before: ghost ink inside the formula's box on 80/160 boards at
  400 x 200 (size 40) and 17/160 at 800 x 400 (size 64). After (margins 0.6
  above/below, 0.9 beside): 0/160 on all three boards; smallest gaps 24
  (400 x 200), 41 (600 x 300) and 42 px (800 x 400). The cost: boards with no
  ghost at all, 24/160 at 400 x 200, 52/160 at 600 x 300 and 0/160 at
  800 x 400 (the wide formulas, the quadratic, the sum and the align, fill the
  smaller boards; the swipes, wipes and smudges still make them look used).
- Perceptual review (tier b), three rounds. Round 1 (four hand-picked
  before/after pairs): PASS. Round 2 (20-seed contact sheets, margins 0.3 /
  0.3): FAIL on the 400 x 200 align: ghosts just past the end of a row read as
  the row going on, so the margins went to 0.35 / 0.9, then 0.6 / 0.9 after
  the sheet still showed ghosts diagonal to the row ends. Round 3: PASS.

### Not done

- A region search is greedy, not optimal: a crowded board can lose a ghost
  that a cleverer packing would keep.
- `gallery/chalkboard_lecture.ps` and `/Ghost` callers of `dcboard` place their
  own ghosts and are untouched.
