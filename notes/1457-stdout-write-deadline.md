## A stdout or stderr write is held to `--max-time` (issue #1457, 2026-11-27)

A program's write to stdout went through std's `write_all` with no
deadline, so `16777216 string =` with stdout a pipe nobody drains blocked
forever with 64 KiB in the pipe, whatever `--max-time` said. The same held
for every writer through `Interp::write_stdout` (`=`, `==`, `print`,
`stack`, `pstack`, `write`/`writestring`/`writehexstring`/`flushfile` on
`%stdout`, `flush`, `printobject`) and for `%stderr` through
`Interp::write_stderr`. Found by Codex on PR #1452.

**What changed.** `src/timed_write.rs` (unix only) writes to a pipe or a
socket so that a wait can be stopped. Under a time limit or a host interrupt
flag, `write_stdout`/`write_stderr` use it for fd 1/2 when that is a FIFO or
socket (`fstat`, once per process):

- FIFO: `poll(POLLOUT, 10 ms)`, then a blocking `write` of at most
  `PIPE_BUF` bytes (512 on macOS, 4096 on Linux). A writable pipe has that
  much room, so the write returns at once.
- Socket: on Linux `send(MSG_DONTWAIT | MSG_NOSIGNAL)` with a `poll` on
  `EAGAIN`. macOS ignores `MSG_DONTWAIT` on a blocking socket (the first
  version of this was caught asleep in `sendto` by the test), so there a
  socket is written like a pipe, 512 bytes after each `POLLOUT` (below the
  2048-byte send low-water mark).
- Between waits, and every 64 KiB written, it asks the interrupt flag and the
  deadline; a stop returns `limitexceeded (time)` or `Interrupted`, both
  uncatchable, so `stopped` cannot swallow it. The callers used to map every
  write error to `ioerror`; they now pass the `PsError` up, and only an I/O
  failure is `ioerror` (a reader that went away still sets
  `stdout_epipe_error`, so the CLI's quiet exit 141 is unchanged).

`O_NONBLOCK` is never set: it is a flag of the open file description, which
the shell and sibling processes share, and setting it makes their writes fail
with `EAGAIN` (the classic "node left my terminal non-blocking"). The cost is
one race: another process writing into the same pipe between our `poll` and
our `write` can fill it, and then that write blocks as before.

**Line buffering.** The timed path writes past std's `LineWriter`, so it
keeps its own `Interp::stdout_pending` (at most 8 KiB): a write goes out
through its last newline, all of it at `flush`/`print`, and a write of 8 KiB
or more goes out at once from the caller's bytes (no copy the memory cap did
not see). `step_n` writes what is left at the end of every slice, so nothing
is pending when the host has control, and the CLI's own output after the run
comes after the program's. Before each timed write std's stdout lock is taken
and its buffer flushed (empty in practice: no system call), so output stays in
order. A drained timed run writes byte for byte what an unlimited run writes
(`a_timed_run_writes_what_an_unlimited_one_does`, a mixed program to a pipe
and to a socket).

**State after a stop.** Bytes already written stay written (the pipe holds a
prefix of the output); the rest of that write, and any held-back partial line,
are dropped. The operator's operands are restored by the central operand
ledger as for any error; the stop is uncatchable, so the run ends.

**What did not change.** No time limit and no interrupt flag (`--max-time 0`
included): the old blocking `write_all`, byte for byte the same code. Stdout
captured (`capture_stdout`: MCP server, wasm playground): unchanged. A regular
file, a terminal or `/dev/null`: the old write; a file does not wait on a
reader, and `poll` on a macOS tty is not dependable. Windows: the old write
(no `poll` on pipes there; the module is `cfg(unix)` and the tests too).

**Deferred.** The CLI's own writes after the run (the error report on stderr,
`--pstack-on-error`, the `pscat: wrote FILE` lines) are still plain blocking
writes: a run stopped with stderr a full, undrained pipe ends its program but
the process then waits on that report. The `%stderr` test uses a slow reader
for that reason. A terminal stopped with XOFF (Ctrl-S) also still blocks.

**Cost** (CPU, best of 5, macOS, release; a guard, not a goal): see the PR
body for the table. With no limit the numbers are base's; with `--max-time`
to a pipe, a line-oriented program pays a `poll` per line and a
16 MiB `print` pays a `poll`+`write` per 512 bytes.
