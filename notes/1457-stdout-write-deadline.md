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

- FIFO on Linux: `poll(POLLOUT, 10 ms)`, then a blocking `write` of at most
  `PIPE_BUF` (4096) bytes. A writable pipe has a free page slot, so the write
  returns at once.
- FIFO on macOS: `fstat` on the write end gives the buffer size
  (`st_blksize`) and the bytes in it (`st_size`), so the write takes exactly
  the free room. `poll` alone is not enough there: macOS starts a pipe at
  16 KiB and grows it to 64 KiB on a write that does not fit, but `poll`
  calls a full 16 KiB pipe not writable. The first version trusted `poll`,
  and an undrained pipe then held 16 KiB where a plain write leaves 64 KiB
  (`==` of a 4 MiB string, whose output `==` bounds, fit on the base and hit
  the limit on the branch). So a full pipe below 64 KiB gets a `PIPE_BUF`
  (512-byte) write, which grows it as a plain write would. That write blocks
  only if the kernel cannot grow the pipe (the system is out of pipe
  memory), the one case where the deadline is not kept on macOS.
  `output_that_fits_the_pipe_is_written_in_full_under_a_time_limit` pins it.
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

**Operand stack after a stop.** The stop is an operator error like any
other, so the central operand ledger restores the operands: `1 2 <4 MiB>
print` stopped at the limit leaves `Operand stack (3): 1 2 (\000...`
(`a_stopped_write_restores_its_operands`), as a loop stopped in `add` leaves
its operands.

**What did not change.** No time limit and no interrupt flag (`--max-time 0`
included): the old blocking `write_all`, byte for byte the same code. Stdout
captured (`capture_stdout`: MCP server, wasm playground): unchanged. The MCP
server's program stderr is not captured and the server runs with a time
limit, so a program's `%stderr` writes there now take the timed path when
the server's stderr is a pipe: the same bytes, but a client that never reads
the server's stderr now sees the run stop at its limit instead of hanging. A regular
file, a terminal or `/dev/null`: the old write; a file does not wait on a
reader, and `poll` on a macOS tty is not dependable. Windows: the old write
(no `poll` on pipes there; the module is `cfg(unix)` and the tests too).

**Deferred.** The CLI's own writes after the run (the error report on stderr,
`--pstack-on-error`, the `pscat: wrote FILE` lines) are still plain blocking
writes: a run stopped with stderr a full, undrained pipe ends its program but
the process then waits on that report. The `%stderr` test uses a slow reader
for that reason. The resource notes in `src/paths.rs` (`PSCAT_TRACE_RESOURCES`, a resource too large) and
`--bands`' own messages are `eprintln!` too. There is no page-to-stdout
output (`--png`/`--svg`/`--pdf` take a file name). A terminal stopped with XOFF (Ctrl-S) also still blocks.

**Cost** (child CPU, min of 7 interleaved with a base-binary control, macOS,
release; a guard, not a goal). With no limit, and to a file with one, head
equals base within the control's spread. With `--max-time` to a drained pipe:

| program | base | head |
|---|---|---|
| `1 1 1000000 { = } for` | 0.381 s | 0.811 s |
| 5M single-byte `write`s to `%stdout` | 0.570 s | 0.610 s |
| 8 x `16777216 string print` | 0.016 s | 0.018 s |

The first is one `fstat` per line on top of the `write` `LineWriter` already
made (about 0.4 us a line); a big write takes the pipe's whole free room per
`write`, so it costs almost nothing extra. On Linux (not measured here) a line
costs a `poll` instead, and a big write is a `poll`+`write` per 4 KiB. Not
tuned: skipping the look while a known free-space credit lasts would also have
to account for every other writer into the same pipe (stderr as `2>&1`, the
REPL prompt), or it would hang in exactly the case this fixes. Hosted runs capture stdout and pay nothing.
