## host API: the gsave depth on Interp (issue #1497, 2026-11-28)

psanim (PR #117) marked the levels it opened with a flatness sentinel and unwound
until it found one, which is unsound (a body that sets flatness to the sentinel
corrupts the restore, 65 open gsaves hit a limit, a caught error leaks the
sentinel). It needs to read the depth of the graphics-state stack before running a
caller's body and after, and carry `after - before` levels across `endgroup`.

**Public API (`pscat::host`, Experimental like the whole crate API).**
- `Interp::gsave_depth(&self) -> usize`: `Gfx::gsave_depth()` (`saved.len()`, the
  count `--lint`'s unbalanced-gsave check already used). Host only: gs has no
  operator for it and none was added. The "is a capture open" half of the issue
  was dropped; `open_group_count` (#1494) covers what psanim needs.

**What moves it** (each pinned in `tests/host_gsave_depth_1497.rs`, read from a
host operator mid-run; the numbers are measured, not derived).
- `gsave` +1, `grestore` -1. `save` pushes **no** level (`do_save` records the
  depth and state in the `SaveRecord`); `restore` truncates to the recorded depth.
- `grestore` floors, where it restores the state and pops nothing: the innermost
  `save`'s depth, a group's own level (inside a group), and 0. `grestoreall` pops to
  the same floors. So `after - before` can be negative.
- `begingroup` calls `gsave()` before recording `Group::depth`, so inside a group
  the depth is `d + 1` (`d` read just before), `d + 1 + k` after the body's `k`
  unmatched `gsave`s, and **`d` after `endgroup`**: `finish_group` truncates to
  `Group::depth` and does one `grestore`. The `k` levels and the group's own are
  lost; the body cannot go below `d + 1` (the group floor), so the landing is
  always `d`. Nested groups each land on their own start. `erasepage`, `showpage`,
  `copypage`, a `restore` past the group, the end of a run, an uncaught error, a
  step limit and the interrupt flag close a group the same way (all land on `d`).
- A `PaintProc` (`begin_pattern` -> `glyph_enter`) and a Type 3 `BuildChar` run one
  level up; `restore_glyph_snapshot` truncates to the depth taken before that
  level, so what they leave open is dropped when they end. The depth is one stack
  across captures: inside a `PaintProc` it includes the page's levels (the opposite
  of `open_group_count`, which reports the page's groups only).
- A caught error (`stopped`, `stop`) does not unwind: levels stay. The end of a run
  and an uncaught error do not unwind either, and nothing resets the depth between
  runs: the next `run_str` starts from where the last left it. Only a group open at
  the end is closed (back to `d`). `showpage`, `erasepage`, `initgraphics`,
  `setpagedevice` and `nulldevice` do not move it.

**Observed and not changed (proposed follow-up).** A `PaintProc` or Type 3
`BuildChar` that `grestore`s more than it `gsave`d is not stopped at its own level
(`glyph_enter`'s level is no floor for `grestore_at`; `group_boundary` needs
`g.captures == self.captures.len()`, so a page group is no floor inside a
`PaintProc` either). `restore_glyph_snapshot` only truncates, so it cannot push the
lost levels back. Measured: `gsave gsave gsave` then a fill with `{ pop grestore
grestore grestore }` reads 3 before, 1 after; with `gsave << >> begingroup` around
the fill the depth in the `PaintProc` goes 3 -> 0 and the page's `endgroup` lands at
0, below its `d` = 1 (`finish_group` skips its truncate once the stack is below the
group's level); a `BuildChar` with three `grestore`s from depth 3 reads 1 after, but
stops at a group's level when one is open. Ghostscript 10.07 raises `unknownerror`
in `pattern_paint_finish` for the `PaintProc` case. Documented on `gsave_depth` as
today's behaviour; pinned in `an_unbalanced_paintproc_or_buildchar_pops_callers_levels_today`
so a fix flips it on purpose.

**Cost.** A read of `&self`: no step, time or memory is charged and nothing is
drawn differently (same `steps_used`, canvas and bytes with 1000 queries per probe,
asserted non-zero under a limit; 100000 queries per call under `--max-steps 40`).
