# #324: drykit headroom for the full chalkboard (2026-10-14)

## Decision

A full, dense chalkboard already fits drykit's limits by a wide margin, so
#324 closes the two correctness gaps and leaves the four speed levers alone.

- **A chunk too big for its catch is split and drawn again.** Before, it
  raised `dkcatch-area-exceeds-safety-limit`.
- **A stroke that no catch could hold is refused when it is recorded.** So
  nothing is drawn first: not the earlier strokes of its batch, and not the
  stroke's own thin start.

Every render that worked before renders byte-identically. The split runs
only where the old code raised, and the up-front refusal fires only where
the old code would have raised anyway.

## Headroom, measured

These are cell counts, not timings. Each run used the release pscat at 72
dpi, with `dkband` wrapped to count catches and their unique cells.

| Board | Catches | Largest catch (cells) | `dkbudget` |
|---|---|---|---|
| `gallery/chalkboard_lecture.ps`, as committed | 1165 | 7,167 (4.8%) | 150,000 |
| a denser 720x480 slate (below) | 65 | 43,093 (29%) | 150,000 |
| a 2,562-stroke batch (below) | 33 | 82,288 (55%) | 150,000 |

- **The denser slate** has seven long `mkalign` lines in one `dcchalk`, a
  `/Tilt 85 /Side 40` curvy side-of-chalk sweep, and a 400-point spiral at
  `/Streak 4` in one subpath. It also renders at budgets of 40,000, 20,000,
  8,000 and 3,000. The largest catch stays at about 0.6 of the budget,
  which is `dkZgroup`'s cut.
- **The 2,562-stroke batch** is a 42-line derivation in a single `dcchalk`,
  so it is one `dkbatch`. It renders in pscat, and in Ghostscript as a
  correctness check. Neither interpreter hit a stack or `limitcheck` error.
  walkpath's segment cap (`wkmaxsegs`) can't trip on a long derivation
  either: `dcchalk` reads the whole path with `pathforall` and calls
  `dkstroke`, and so `walkpath`, once per subpath. Only one glyph stroke's
  segments count against the cap, at any scale. A batch has no practical
  stroke limit.
- **The lecture** renders byte-identically on this branch. It takes about
  6.5 s CPU, against DRYMEDIA 6.2's 30 s. That is one run on a shared
  machine, offered as a guard, not a benchmark.

The limit is reachable, though. A sweep of wiggles and arcs (widths 1 to
12, curve radii 2 to 16, `/Streak` 1 to 4, budgets 400 to 3,000) raised on
83 of 192 strokes on main.

- Some raised where any single stop fits easily. At budget 3,000, a
  `/Width 12` wiggle's largest stop needs about 800 cells.
- In those, the estimate `dkZgroup` makes before `dkZthin`, plus the
  overlap, came out short.

At the default budget a chalk-sized stroke never gets near this. But a
board that grows by hand should never meet it.

## What shipped (lib/drykit.ps)

- **`dkZdraw` splits a rejected chunk.**
  - **Detecting the overflow.** `dkcell` sets `userdict /dkOver true`
    just before it raises on overflow, so the hot path pays nothing.
    `dkZdraw` clears the flag and wraps only `dkcatch` in `stopped`. On an
    overflow it pops the operand stack back to its depth before the catch.
    A failed `undefined` leaves the name there, and `dkZgroup`'s mark and
    pending entries sit underneath. It then gives back its `gsave`, resets
    `$error /newerror`, and draws both halves outside the `stopped`.
  - **How `dkZsplit` splits.** Two or more entries split into halves. A
    one-entry run splits into two pieces that share the middle stop, as
    `dkZgroup`'s pieces do. A single segment longer than a cell is cut at
    its midpoint, with the half-width interpolated. Ink drawn with a
    constant width is unchanged by that cut.
  - **What can't be split.** A dot, or a segment within a cell, re-raises
    with `$error` intact.
  - **Why it is safe.** A rejected band changes no graphics state
    (`dkband`'s own contract), so retrying is safe. `dkZsplit` reads none
    of `dkZgroup`'s in-flight scratch.
- **`dkSrecord` refuses up front.** It runs after the runs are resolved,
  so lanes, `/Tilt`, `/Pressure` and `/Wear` are already in the
  half-widths.
  - **The bound.** For each run that catches (touch below 1), it takes the
    widest stop's reach at that run's touch: dkband's `dkK`, the top
    level, no halo, no `dkslack`. It subtracts a cell's half-diagonal.
    Every cell that meets the smaller disc has its centre inside the
    reach, so `pi r^2 / W^2` is a true lower bound on the cells any catch
    over that stop builds.
  - **Thinned and split catches cover it too.** Each kept stop's reach
    grows by the window's largest `deviation + half-width`. That covers
    every original stop's disc and every original segment's capsule at
    its narrower end.
  - **The check.** If the bound passes `dkbudget`, it raises
    `dkcatch-area-exceeds-safety-limit` (the same name as before) before
    anything is drawn. It is skipped in the whole-page test mode, whose
    cap is not `dkbudget`.
  - **What it costs.** One pass over the record's stops.
- **The gap between the bounds.** Between the up-front bound and a dot's
  real cell count lies a sliver about a cell's half-diagonal of reach
  wide: about 0.3 pt of 87 pt on slate at the default budget. A stroke in
  that sliver still raises at render, with the `gsave` given back. On main,
  earlier strokes and levels would already be drawn. In the hunt at tiny
  budgets, 16 strokes fell in it. Every other failure there was refused
  up front, and every stroke that used to raise only because its chunk
  was too big now draws. A stroke in the sliver is slow to fail: each
  retry builds up to `dkbudget` cells before it is rejected, and it can
  take a dozen or so retries before the guard is raised. At the default
  budget that can take tens of seconds. It is not a hang.
- **The dict stack needs no unwinding.** Nothing between `dkcatch` and the
  overflow in `dkcell` does a `begin`.

## Evidence

- **`tests/drykit.rs`, `a_chunk_past_the_budget_is_split_and_drawn`.** One
  row is a curvy `/Width 12` run at budget 3,000, where pieces are cut down
  to single segments. The other is 24 small circles in one batch with
  `dkZest` made to see nothing, so the entries are halved.
  - Each row must take the split (a counter) and land within 3% of the
    default-budget render's ink.
  - Against main's drykit the first row raises.
- **`tests/drykit.rs`,
  `a_stroke_too_wide_for_any_catch_is_refused_before_anything_draws`.** Two
  rows: a thin stroke, then a `/Width 30` one, in a batch; and one stroke
  whose `/Pressure` widens it near its end, drawn at once.
  - Both raise the guard name and leave no mark at all (luma < 250).
  - Against main's drykit, the batch leaves the first stroke and the
    at-once stroke leaves 328 marked pixels of its thin start.
- **Self-tests.** `dkstroke-a-run-past-the-budget-raises-cleanly` gains a
  `dkslack 400` row. The slack is render-only reach that the up-front
  check leaves out, so the run is split down to sub-cell pieces and
  `dkcatch` raises. The row asserts that the caller's colour survives.
  With the handler's `grestore` cut out, it fails and reports "5 unmatched
  gsave(s)". `dkZsplit-halves-a-rejected-chunk` pins each split case.
- **Ghostscript (correctness only).** The split path renders under gs with
  the same split count as pscat (2), and an operand-stack sentinel
  survives. The up-front refusal raises the same name. The 2,562-stroke
  batch renders.
- **Identity.** `scripts/identity_check.sh`, with `--loader` for drykit,
  drychalk, drycharcoal, drycrayon and mathkit, and `--files` for the dry
  gallery pieces and the DRYMEDIA prototypes. Result: see the PR. The
  control is a wiggle that only renders with the split.

## Not done: the four speed levers

#283 carried four speed levers into #324: a reach band per knuckle
orientation, fewer revisits in `dkcell`, one walk pass in `dkSrecord`, and
the `pjheight` memo cap. None was attempted.

- **The board doesn't need them.** It is at about 7 s of a 30 s budget,
  and its largest catch uses 5% of `dkbudget`. The maintainer treats
  performance as a guard, not a goal.
- **Renders must stay byte-identical.** A tighter band per orientation
  would have to keep every knuckle that shades an edge pixel of the ink,
  and every cell's nearest-segment orientation, exactly.
  - Knuckles that end within a pixel of the ink still shade it, which is
    why `dkslack` exists.
  - Which segments reach a cell decides its orientation.
  - Holding both exactly might be possible, with about 1.5 px of slack,
    but that has not been measured.
- **The issue's own fix for the estimate gap was not taken.** It proposed
  ending a thinning window when the deviation passes 0.25 W. That moves
  the thinned stops, and with them knuckle orientation on the real board.
  Splitting on rejection closes the same gap and changes nothing that
  renders today.
- **No ticket.** These are speed work with no consumer. If a later board
  needs the time, the data is here and in
  `notes/283-drykit-perf-remeasure.md`.
