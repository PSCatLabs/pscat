## chalk: ghost writing keeps out of the formula's box (issue #698, 2026-10-25)

`pscat chalk`'s board dressing draws two or three faint ghost formulas, the
eraser's leftovers of an earlier lecture. They were placed in `src/chalk.rs`
from the seed alone, in bands along the top and bottom of the board, so they
did not know where the writing was. On a wide formula they collided on every
seed (#555's cold reviewer read a ghost beside `f(x)` as a superscript; #699's
agent turned `board.ghosting` off for its review images). Measured before the
fix, ghost-only footprint against the formula's ink box, board dressing off,
600 x 300 at size 64: 20 of 20 sweeps had ghost pixels inside the formula's
box.

### What changed

- `ghost_placement` (was `ghost_paths`) still draws the dice in Rust (which
  formulas, nominal sizes, a `u`/`v` pair and an order of regions per ghost)
  but no longer places anything. The writing's extent exists only after the
  driver's measuring pass, so the placing is PostScript: `GHOST_PLACER`, in
  `ckdict` just before `dcboard`.
- The writing's box is the bbox of the path the writing is inked along
  (`mkalign` with no `/Ink`, then `pathbbox`), not `ckx0..cky1`: those are the
  line boxes and the ink strays past them (12-16 pt on an align at 400 x 200).
  It is inflated by 0.3 of the writing size on every side.
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
  needs a gap of 0.3 of the writing size.
- A pixel sweep (ghost-only render minus a ghost-less one, against the
  `--no-board` render's alpha box) over 8 formulas x 20 seeds: see the PR.
- The five pinned boards in `tests/chalk.rs` and the `chalkboard_math` pin in
  `tests/mcp.rs` moved (the ghosts did); re-pinned.

### Not done

- A region search is greedy, not optimal: a crowded board can lose a ghost
  that a cleverer packing would keep.
- `gallery/chalkboard_lecture.ps` and `/Ghost` callers of `dcboard` place their
  own ghosts and are untouched.
