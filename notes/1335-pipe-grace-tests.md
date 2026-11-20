## The 2 s pipe-grace window is gone from the test helpers (issue #1335, 2026-11-19)

`a_postscript_error_fails_the_exit_code`, the `tests/subcommand_limits.rs`
tests and `fonts_list_stops_quietly_when_its_stdout_reader_goes` failed on a
loaded Mac with "a descendant of the child still held its output pipes 2s after
the child exited". The same family as #1294 and #1201 (reader closed too late),
#1118 and #1233 (pipe timing).

**What the old check did.** `tests/common/mod.rs` drains a child's stdout and
stderr on threads. After the child exited it waited at most `PIPE_EOF_GRACE`
(2 s) for each thread to see EOF, and failed the test if it did not, on the
theory that a descendant of the child held the pipe. That is a wall-clock
window on a machine the test does not control.

**What it was probably not.** None of these children spawn anything (`src/`
starts processes only in the MCP server, `mkfifo` and `curl`), so the helper's
list of descendants seen would have been empty. The pipe was then not held by a
runaway child: EOF was late for some other reason (a drain thread not scheduled on a starved machine,
or another process that had inherited the write end). The cause on the failing
machine was not pinned down: it had been up 22 days and its pipes held 512
bytes (`notes/1118-pipe-timing-tests.md`).

**Not reproduced.** After a reboot (this Mac, 2026-11-19): 8 rounds of the full
`cli` and `subcommand_limits` binaries on the `ci` profile under 24 busy
processes, then 15 rounds of the flaky tests alone under 40 busy ones: 0
failures. A scratch program (4 threads spawning 1 s children with piped stdio
while 4 more spawned `true` and timed EOF) saw no inherited-fd delay either,
worst EOF 17 ms. So the fix is chosen by what the check should be, not by a
before/after count.

**The fix.** `collect` waits for EOF, with no fixed window. It stops waiting
only when
1. a descendant that was seen alive while the child ran (the existing
   `pgrep -P` snapshot) is still alive 2 s after the child exited (the runaway
   the check exists for; `tests/child_deadline.rs` keeps proving it), or
2. the test's own hang guard (`deadline`, 60 s by default) passes.

With no known descendant the 2 s window does not exist, so a late EOF only
delays the test. The 20 ms receive interval is a polling step, not a deadline.
`tests/cli.rs` pins both halves on `collect` itself, with no process timing
(a first version ran `sh -c '(sleep 3 &); ...'` and was dropped: on a starved
machine the 200 ms snapshot could catch the `sleep` as a suspect and the test
would flake like the thing it tests): a pipe whose EOF comes after 3 s is
waited for, and a live suspect is given up on after about 2 s. Negative check
done once with that first version against the old helper: it failed with
"a descendant of the child still held its output pipes 2s after the child
exited (killed the ones seen: [])", the new helper passed it.

The 2 s window remains for a descendant seen alive, because
`tests/child_deadline.rs` (not in this change's lane) proves the runaway
detection through it. Both pipes now share one hang-guard budget counted from
the child's exit.

**Load check of the new helper (2026-11-19).** The built `cli`,
`child_deadline` and `subcommand_limits` binaries, 20 rounds each under 8 busy
processes: 0 failures. `child_deadline`'s descendant test now rests on
`pid_alive`, not a clock, and passed every round.

Known limits, from the review: a descendant started after the last `pgrep`
snapshot (the first is taken at 200 ms) is never a suspect, so a runaway like
that fails at the hang guard instead of after 2 s; a suspect pid that is a
zombie or was reused counts as alive; the 2 s is counted from the first poll of
the stdout wait, a moment after the child exit. A drain thread that ends
without sending fails the run rather than reading as an empty pipe.

The closed-reader tests already close the read end before spawning (#1294);
nothing in the three tests named in the issue needed more than the helper.
`--max-time 0.3` in `subcommand_limits.rs` is the child's own limit under test
(it stops the runaway whatever the load), not a window the test waits on.
