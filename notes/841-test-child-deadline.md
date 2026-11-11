## Test helpers kill a hung child after a deadline (issue #841, 2026-10-26)

A `pscat --max-time 0.5 -e { } loop` child left over from #604 test-first work
(run against a binary without the limit wired up) spun for 2.8 days: the
integration tests called `wait_with_output()` / `output()` with no deadline, and
the orphan outlived the interrupted runner.

## What changed

`tests/common/mod.rs` gains std-only helpers (plus `libc::kill`, already a
dev-dependency, for the descendant kill):

- `Command` extension trait `CommandDeadline`: `output_deadline()`,
  `status_deadline()`, `output_with_input(bytes)` and `*_within(duration)`
  variants. They return `io::Result` like std, so `.expect()`/`.is_err()` call
  sites change only in the method name. A deadline is a panic, after the child
  is killed and reaped.
- `wait_with_deadline(child, d)` / `wait_output(child)` for children a test
  spawned and fed itself.
- `Watchdog::new(&child, d)`: a guard thread for the long-lived MCP `Server`
  (blocking `read_line`): after the deadline it kills the server, the pipe hits
  EOF and the test fails instead of hanging. Dropping it cancels at once.

Details that matter:

- stdout and stderr are drained on threads and stdin is written on a thread, so
  a full pipe cannot deadlock the child. The wait is `try_wait` polling with a
  1-25 ms back-off, so a fast child costs about a millisecond.
- On the deadline, descendants are killed too (`pgrep -P` walk, by parent pid,
  never a name pattern): `pscat-mcp` runs every render in a pscat child of its
  own, which a plain `Child::kill` of the server would orphan.
- The child stays in the runner's process group on purpose. `process_group(0)`
  would stop Ctrl-C of `cargo test` from reaching it, which is the other way
  the original leak happened.
- Default 60 s: it exists to stop a hang, not to time a render, and gates run
  with load averages above 30 (a slow render is far under it). `PSCAT_TEST_DEADLINE_SECS` overrides it.

## Scope

Every test that runs `pscat` / `pscat-mcp` (CLI, `--max-*`, subcommands,
`tests/subcommand_limits.rs` (its hand-rolled loop is gone), the MCP `Server`,
`pscat_mcp_never_writes_files`) uses them. Ghostscript, python3, node, mkfifo,
`ps` and the build scripts are other tools with their own failure modes and
are left alone. A held-open-stdin case in `tests/subcommands.rs` keeps its own
10 s loop.

## Proof

`tests/child_deadline.rs`: a deliberate `{ } loop` with a unique marker in its
argv is killed at the deadline through each helper, the test fails with the
command in the message, and `pgrep -f <marker>` finds nothing afterwards
(a control shows pgrep does see the marker while it runs). A shell that
backgrounds the runaway proves grandchildren die too. Large stdin/stdout/stderr
transfers prove there is no pipe deadlock. A scan test fails on a raw
`wait_with_output()` or an undeadlined `Command::new(<pscat binary>)` `.output()`
/ `.status()`.

## Descendants that outlive the child (Codex review, P2)

A child that exits while a backgrounded descendant still holds its stdout or
stderr used to look like success with empty output. Now, if a pipe is not at
EOF 2 s after the child exited, the helper kills the descendants it last saw
(a `pgrep -P` snapshot taken from 200 ms into the run, then every second, so a
quick child pays nothing) and fails the test. A descendant that appears and is
orphaned within the first 200 ms is reported but cannot be found to kill.

## Windows

`kill_tree` and `Watchdog` are no-ops off unix (no `pgrep`/`libc::kill`); the
`output_*`/`status_*`/`wait_*` helpers still kill the direct child there.
`tests/child_deadline.rs` is `#![cfg(unix)]`. Only the unix runners gate merges.

## Not solved

If the test runner itself is SIGKILLed, no in-process deadline can fire; the
child only dies if it shares the runner's terminal Ctrl-C. A `PR_SET_PDEATHSIG`
style parent-death guard would need a `pre_exec` hook (Linux only) and was not
added.
