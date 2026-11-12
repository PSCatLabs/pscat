## restore brings back setpacking; empty-target runs are bounded (#1124, #903, 2026-11-11)

One bundle, two small interpreter changes.

### #1124: `restore` and `setpacking`

`false setpacking /s save def true setpacking s restore currentpacking =`
printed `true`; Ghostscript 10.07.1 prints `false`. `SaveRecord` now carries
the `setpacking` flag as `do_save` found it, and `do_restore` puts it back
(after the record is chosen, so a restore of an outer save takes the outer
save's mode, not an inner one's). Checked against `gs` on six programs: a
plain pair both ways, an outer restore over two newer saves both ways, an
inner restore followed by the outer one, and an unrestored save. All six agree.

PR #1103 (#733) does the same for `setobjectformat`. The two changes sit in
different places in `SaveRecord`, `do_save` and `do_restore` on purpose, so
either merges first and the other applies without a conflict (checked with
`git merge-tree` against #1069, #1070, #1103, #1126 and #1128). The test is
in `tests/control.rs` (`restore_brings_back_the_packing_mode`), next to the
other `save`/`restore` tests, since #1103's `tests/binobj.rs` is not on main.

### #903: the other empty-target loops in `next_item`

#900 bounded the three `Iterate*` arms with `EMPTY_RUN`. Two more arms did
synchronous work in a row inside one call: `Action::ExecWith` whose target is
an empty procedure (`pathforall` with `{ }` procs, `kshow { }`, an image data
procedure) and `Action::Nothing` (a show frame's `ShowStep::Again`, one glyph
painted per round). Each is bounded by its path, string or image, so nothing
hangs, but a huge input made one step arbitrarily long, which costs window and
spool responsiveness.

Both arms now count into the same `empty_run` local and return
`Item::Proc` at `EMPTY_RUN` (1024), as the loop arms do; the step loops treat
that as a step that did nothing. The check happens after the target has run
(a no-op) and the operands are pushed, so the `PathForall` or `Show` frame
is still on top and has advanced. The counter is shared by all arms, so a
mixed run still pauses once per 1024. An `ExecWith` whose target is not an
empty procedure (an operator or name, or a non-empty procedure) is not
counted: a procedure yields on its own, and a bare operator target is not
what this issue lists. An image data procedure that is empty leaves nothing
for the next `supply`, which errors, so it is bounded already.

`next_item` has two callers, `step_n_inner` and `step_n_timed`, and both
already treat a `Proc` item with no procedure on top as a step that did
nothing.

Output is byte-identical: only the point where `next_item` hands control back
changes.

**Step counts.** Up to 1024 in a row are free, as before. A longer run costs
one extra step per 1024 (`3000 show` 2 -> 4, a 3000-lineto `pathforall` with
empty procs 5 -> 7, `kshow` 3 -> 5). Pinned in
`a_run_of_empty_targets_is_one_step_per_1024`, with the values for 100 and
1023 equal to the tree before the change. The step-cap accounting is
as for #900's pauses: a pause is a step, so under `--max-steps` a long show
or `pathforall` of empty procedures costs one extra step per 1024 glyphs or
elements (`tick` is not involved, and the loop iterations' accounting is
unchanged).

**Tests (written first; the slice test failed with "ended early" on the old
tree).** `a_slice_returns_on_every_kind_of_empty_target_over_a_huge_input`
sets a 60000-segment path and a 60000-byte string up with `run_str`, then
runs `{ } { } { } { } pathforall`, `{ } s kshow` and `s show` for three
`step_n(10)` slices in both step loops.

### Identity and instructions retired

`scripts/identity_check.sh --base-bin/--head-bin --loader lib/artkit.ps`:
125 page renders across 94 files, all identical; the negative control
(`true setpacking save false setpacking restore currentpacking { rectfill } if`,
which draws only with the #1124 fix) differs between the trees, as it must.

`/usr/bin/time -l` instructions retired, whole run, base vs head (2M-element
programs; noise between repeats is about 1.5%, so these are level):

| program | base | head |
|---|---:|---:|
| 10 x `{ } { } { } { } pathforall` over 200k segments | 1.91G-1.94G | 1.93G-1.95G |
| 10 x `show` of a 100k-char string | 11.96G | 12.00G |
| 10 x `{ } kshow` of a 100k-char string | 12.66G | 12.74G |
| `1000000 { } repeat` | 97.7M | 94.0M |
| `fib 27` | 1.2635G | 1.2651G |

### Not done

`setglobal` is undefined in pscat (Ghostscript's `false setglobal save true
setglobal restore currentglobal =` prints `false`), so there is no global-VM
mode to save.
