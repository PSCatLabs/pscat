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
host operator mid-run; the numbers are measured on a running binary, not derived).
`d` is the depth read just before a `begingroup`. Codex round 1 corrected the
first version of these rules (D1, D2, D3 below).
- `gsave` +1, `grestore` -1. `save` pushes **no** level (`do_save` records the
  depth and state in the `SaveRecord`).
- `restore` and `grestoreall` with a `save` live **only truncate**
  (`restore_glyph_snapshot` calls `saved.truncate(record)`): the depth becomes
  `min(current, record)`, never higher. `restore` closes every group opened since
  the `save`. `grestoreall` first asks `grestore_all_in_group`: a group opened
  since the `save` stops it at the group's level and is not closed. (D2) A record
  can be above the depth when something removed levels beneath it, e.g.
  `gsave << >> begingroup save endgroup restore`: record 2, depth 1 after
  `endgroup`, still 1 after `restore` (and after `grestoreall`).
- `grestore` floors, where it restores a state and pops nothing
  (`grestore_at`: `saved.len() <= record`, or a group's level): the innermost
  `save`'s record, a group's own level (only a group opened since that `save`
  counts), and 0; the higher applies. A record above the depth makes `grestore`
  pop nothing until the depth is above it. `grestoreall` pops to the same floors.
  So `after - before` can be negative.
- `begingroup` calls `gsave()` before recording `Group::depth`, so inside a group
  the depth is `d + 1`, `d + 1 + k` after the body's `k` unmatched `gsave`s, and
  **`d` after `endgroup`**: `finish_group` truncates to `Group::depth` and does one
  `grestore`. The `k` levels and the group's own are lost. The body cannot go below
  `d + 1` by `grestore`, so the landing is `d`, unless the body `restore`s a `save`
  made before the group (below) or a `PaintProc` over-restores (below).
  `erasepage`, `showpage`, `copypage`, the end of a run, an uncaught error, a step
  limit and the interrupt flag close a group the same way (all land on `d`).
  (D1) A group closed by a `restore` lands on the `save`'s record, which is `<= d`:
  `gsave save gsave << >> begingroup gsave restore` has `d` = 2 and ends at 1
  (3 in the group, 4 after the `gsave`). Two groups opened since a `save` are both
  closed by its `restore`.
- A `PaintProc` (`begin_pattern` -> `glyph_enter`) and a Type 3 `BuildChar` run one
  level up; `restore_glyph_snapshot` truncates to the depth taken before that
  level, so what they leave open is dropped when they end. The depth is one stack
  across captures: inside a `PaintProc` it includes the page's levels (the opposite
  of `open_group_count`, which reports the page's groups only).
- (D3) A caught error (`stopped`, `stop`) does not unwind the levels `gsave`d between
  the `stopped` and the error, **except** that an error escaping a `PaintProc` or
  `BuildChar` unwinds that callback (`abort_pattern` / the glyph's
  `restore_glyph_snapshot`) and removes its levels even when an outer `stopped`
  catches it: Codex's program reads 1 before, 4 in the callback, 1 after; a
  `BuildChar` the same. A `stopped` inside the callback is no escape: the levels
  stay until the callback ends. The end of a run and an uncaught error leave the
  levels outside any callback, and nothing resets the depth between runs: the next
  `run_str` starts from where the last left it. Only a group open at the end is
  closed (back to `d`). `showpage`, `erasepage`, `initgraphics`, `setpagedevice`
  and `nulldevice` do not move it.

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
