## Empty loop bodies pause every 1024 iterations (issue #900, 2026-10-26)

**The bug.** With no limits set, `Interp::step_n(n)` never returned on
`{ } loop`. The window and `--spool` drive the interpreter a slice at a
time from the event loop, so the window froze: no redraws, no Esc.

**Why.** A loop frame's iteration (`Action::Iterate`, `IterateWith`,
`IterateWith2` in `next_item`) pushes the body's procedure frame and goes
round `next_item`'s loop again. A non-empty body then returns `Item::Proc`,
and its first element runs as that step. `push_proc_frame` pushes nothing
for an empty body, so `next_item` goes straight on to the next iteration
and never returns. `step_n` never gets to decrement its slice.

Every loop kind with an empty body has this shape: `loop`, `repeat`, `for`
(integer and real control), and `forall` over arrays, strings and dicts.
A finite loop finished inside one step, which is why `3 { } repeat` takes
3 single steps (pinned since #875). An endless or huge one hung. Loops
whose body has anything in it are bounded already, fused `if`/`ifelse`
included, since each element is a step: `{ true { } if } loop` and
`{ { } loop } loop` behave the same.

`--max-steps` and `--max-time` still stopped it because every iteration
calls `tick`. Every `POLL_PERIOD` (1024) iterations its cold `poll_slow`
charges the steps and reads the clock, but only when limits are set.
With none it returned `Ok` and the loop went on.

**The fix.** The three `Iterate*` arms check for an empty body themselves
and count the empty iterations in a local of `next_item`. At `EMPTY_RUN`
(1024) in one call, `next_item` returns `Item::Proc` with the loop frame
still on top. Both step loops (`step_n_inner`, and `step_n_timed` for
`--profile-ops`) already treat a `Proc` item with no procedure on top as
`ProcStep::Other` and `continue`, so the pause is one step that did
nothing. No new `Item` variant and no new arm in the hot loop.
`push_proc_frame` keeps its own empty check for its other callers (`exec`,
the conditionals, `ExecWith`).

**What changed in step counts.** Nothing that was pinned. Up to 1024
empty iterations in a row are still free. A longer run costs one step per
1024 (`1024 { } repeat` 3 -> 4 single steps, `4096 { } repeat` 3 -> 7).
These are pinned in `a_run_of_empty_iterations_is_one_step_per_1024`.
`tick` still runs on every iteration, so the `--max-steps` accounting is
unchanged (`the_step_cap_counts_loops_as_before` passes untouched).

**Tests (written first, both hung on the old tree).**
- `tests/dispatch_semantics.rs` `a_slice_returns_on_every_kind_of_long_loop`:
  `step_n(1000)` returns `Ok(true)` three times, in both step loops, for
  `{ } loop`, a bound one, a huge `repeat`, integer and real `for`, a 10M
  string `forall`, nested empty loops, fused `if`/`ifelse` bodies, and a
  non-empty control.
- `src/window.rs` `an_endless_empty_loop_hands_back_every_frame`: a spool
  app with no limits runs `{ } loop`, and `App::tick` returns 200 times
  with the job still running.

Both run under a 60 s in-process deadline (`within_deadline`, as in #895).

### Instructions retired (`/usr/bin/time -l`, best of 3)

The groups run 5 copies per iteration of a bound 1M `for`, less the empty
loop. The iteration rows are per iteration, less start-up. Code layout
alone moves a row by about ±6 (#864).

| row | base (41e3040) | head |
|---|---:|---:|
| `1 pop` | 157 | 154 |
| `1.5 2.5 mul pop` | 341 | 334 |
| `x y mul pop` | 549 | 540 |
| `true { 1 pop } if` | 652 | 646 |
| `0 1 2 { pop } for` | 1579 | 1543 |
| `1 { 1 pop } repeat` | 786 | 773 |
| `3 { } repeat` | 645 | 623 |
| `1 1 3 { } for` | 1203 | 1163 |
| `for` iteration, `{ pop }` | 271 | 270 |
| `repeat` iteration, `{ 1 pop }` | 273 | 273 |
| `forall` iteration, `{ pop }` | 379 | 381 |
| `repeat` iteration, `{ }` | 43 | 41 |
| `fib 27` | 770.5M | 761.9M |
| `gallery/basin_and_range.ps` | 19.20G | 18.86G |

Nothing got dearer. The small drops, basin_and_range's 1.8% included, are
most likely layout: the hot loop's code did not change. The basin_and_range
PNG is byte-identical.

### Left alone

`Action::ExecWith` with an empty target also loops inside `next_item`
without yielding: `pathforall` with empty procedures, `kshow` with `{ }`,
and an image data procedure. Each is bounded by its path, string or image,
so none hangs. Bounding them as well would mean the same counter in that
arm. `Show` and `Image` belong to the image and filter lane (#896).
