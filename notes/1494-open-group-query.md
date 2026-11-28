## host API: open transparency groups on Interp (issue #1494, 2026-11-27)

psanim implements its layers with `Interp::swap_canvas`, which while a
transparency group is open (#1406) acts on the page below the group's layer. To
know whether a group is open it counted `begingroup`/`endgroup` calls in its
prelude, which misses a group opened through `systemdict /begingroup get exec`
and one that `erasepage`, `showpage` or `restore` ends, and (second ask, from
Codex's review of psanim PR #117) cannot tell that a caller's procedure closed
the group and opened another at the same depth.

**Public API (`pscat::host`, Experimental like the whole crate API).**
- `Interp::open_group_count(&self) -> usize`
- `Interp::innermost_group_serial(&self) -> Option<u64>`

**Semantics.**
- Both are the page's: `Gfx::groups` entries with `captures == 0`. `groups` is a
  stack and a group opened inside a pattern capture has `captures >= 1`, so the
  page's groups are a prefix (`take_while`). Inside a `PaintProc` they report the
  page's groups, not the `PaintProc`'s own (a tile's, which `swap_canvas` cannot
  reach). A `PaintProc`'s `endgroup` with none of its own open is already a no-op
  (`end_group` closes a group only when `g.captures == captures.len()`), so the
  answer cannot change because of it. With no page group open the count is 0
  inside a `PaintProc` although `swap_canvas` returns the page there (no page to
  exchange): 0 means "no group below", not "swap works".
- `count > 0` is "`swap_canvas` addresses the page below a group" with one
  exception: a group opened while nothing paints (`nulldevice`) has no layer
  (`below == None`); it is counted (as `endgroup` counts it) and displaces
  nothing. Said in the doc and pinned (`a_group_opened_under_nulldevice_is_counted`).
  A separate boolean mirroring `page_slot_mut` was not added.
- The serial is `Group::serial`, taken from `Gfx::groups_opened`, which `begin_group`
  bumps only for a group that opened (a refused one, `limitcheck` at depth 64 or
  at the pixel cap, takes none). The counter is never reset or saved: not by
  `restore`, `grestore`, a page boundary, `setpagedevice`, the end of a run or an
  error. Serials start at 1; the same group keeps its serial, a closed-and-reopened
  one has a later one. A `PaintProc`'s group takes a serial from the same counter
  (not reported).
- What closes a group (all unchanged by this ticket, now observable): `endgroup`;
  `erasepage`, `showpage`, `copypage`; a `restore` past it; the end of a run, an
  uncaught error, a limit and the interrupt flag (`end_open_groups` /
  `unwind_all`). A caught error (`stopped`) does NOT close it, as it does not close
  a `gsave`: the doc says so. Between slices of `begin_source`/`step_n` a group stays
  open.

**Cost.** Pure reads on `&self` of at most 64 `Group`s (`MAX_GROUP_DEPTH`); no step,
time or memory is charged and nothing is drawn differently
(`tests/host_open_groups_1494.rs`: same `steps_used` and canvas with and without
the queries, and under a 40-step limit with 100000 queries per call). No rendering
changes: `scripts/identity_check.sh` with the base and head binaries (PR body).

**Tests.** `tests/host_open_groups_1494.rs` reads both from a host operator
(`probe`) inside the program: the `systemdict` route, `//begingroup` in a bound
procedure, `erasepage`/`showpage`/`copypage`/`restore`, nesting, a caught error, an
uncaught error, a step limit and the interrupt flag, a group left open between
slices, a refused `begingroup`, `nulldevice`, three `PaintProc` cases, the serial
across `restore`, page boundaries and runs, and `swap_canvas` itself (the swapped-in
page shows the paint only after `endgroup` while a group is open).

**Requirements relayed from the psanim session (2026-11-27).** Count and id are the
page's (above); any route counts (`systemdict`, `//begingroup`, a bound procedure;
tested); `copypage` is in the closers test; a `PaintProc`'s `endgroup` leaves both
unchanged (tested); the id is the group's own, taken when it opens, so inner groups
opened and closed in a body leave it alone
(`a_group_keeps_its_own_id_while_inner_groups_come_and_go`); both are `&self`, and
every test reads them from inside a host operator mid-run with groups open (the
`probe` operator; the doc tests do the same). One point where the wording differs
from the interpreter: an error that is *caught* (`stopped`) does not end a group
(neither does it end a `gsave`); only an uncaught one does, with the run. Documented
and pinned (`error_recovery_closes_by_the_interpreters_own_rules`).
