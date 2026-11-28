## A stdout or stderr write is held to `--max-time` (issue #1457, 2026-11-27)

A program's write to stdout went through std's `write_all` with no
deadline, so `16777216 string =` with stdout a pipe nobody drains blocked
forever with 64 KiB in the pipe, whatever `--max-time` said. The same held
for every writer through `Interp::write_stdout` (`=`, `==`, `print`,
`stack`, `pstack`, `write`/`writestring`/`writehexstring`/`flushfile` on
`%stdout`, `flush`, `printobject`, `writeobject`) and for `%stderr` through
`Interp::write_stderr`. Found by Codex on PR #1452.

### The write

`src/timed_write.rs` (unix only) writes so that a wait can be stopped.
Under a time limit or a host interrupt flag, `write_stdout`/`write_stderr`
use it for fd 1/2 when that is one of these (looked at once per run, so a
host may move the descriptors between runs):

- A pipe, Linux (anonymous or named): `pwritev2(..., RWF_NOWAIT)`,
  non-blocking for that one call, with exactly a plain write's room; `poll`
  on `EAGAIN`. `poll` alone undercounts the room: it calls a pipe full once
  its 16 page slots are in use, while a small write still fits in the last
  page (Linux CI caught an undrained pipe holding 61,400 of 65,536 bytes). A
  kernel that refuses the flag for pipes falls back to `poll`, then a blocking
  `write` of at most `PIPE_BUF` (4096) bytes, which a free slot takes at
  once.
- An anonymous pipe, macOS: `fstat` on the write end gives the buffer size
  (`st_blksize`) and the bytes in it (`st_size`), so the write takes exactly
  the free room. `poll` alone is not enough there: macOS starts a pipe at
  16 KiB and grows it to 64 KiB on a write that does not fit, but `poll`
  calls a full 16 KiB pipe not writable (the first version trusted it, and
  an undrained pipe then held a quarter of what a plain write leaves). A full
  pipe below 64 KiB gets a 512-byte write, which grows it as a plain write
  would.
- A stream socket (`SO_TYPE` is `SOCK_STREAM`): on Linux
  `send(MSG_DONTWAIT | MSG_NOSIGNAL)` with a `poll` on `EAGAIN`; on macOS,
  which ignores `MSG_DONTWAIT` on a blocking socket (a test caught `sendto`
  asleep), 512 bytes after each `POLLOUT` (a writable macOS socket has its
  2048-byte low-water mark free).

Everything else keeps the old blocking write: regular files, terminals
(`poll` on a macOS tty is not dependable), `/dev/null`, datagram and
seqpacket sockets (splitting a write would change the message boundaries:
`a_datagram_stdout_gets_the_same_datagrams_under_a_time_limit`), and on
macOS a named FIFO. A macOS named FIFO is socket-backed: `fstat` reports
size 0 and `st_blksize` 4096 whatever it holds, and `poll` calls it writable
with one byte free, so no write size is safe; it is told from an anonymous
pipe by `st_dev` (set for a FIFO on a file system, even once unlinked; 0 for
`pipe()`). Checked by hand: an undrained named FIFO, linked or unlinked,
blocks past `--max-time` as before.

`O_NONBLOCK` is never set: it is a flag of the open file description, which
the shell and sibling processes share, and setting it makes their writes
fail with `EAGAIN`.

The write asks the interrupt flag and the deadline before its first byte,
whenever it would wait (it polls every 10 ms), after an `EINTR`, and every
64 KiB written (also after the last chunk). A stop returns
`limitexceeded (time)` or `Interrupted`, both uncatchable. A zero-byte
`write` is `WriteZero`, as in std. A hung-up pipe (`POLLHUP`/`POLLERR`) is
written to, so the `EPIPE` comes back as from a plain write: a full pipe
whose reader closes ends in 2 ms with the quiet exit 141.

### Stdout's line buffer (Codex round 1, I1-I4)

std's stdout is a `LineWriter` of 1024 bytes, and where it flushes decides
how stdout and stderr interleave (`2>&1`) and in which operator a write
error surfaces. The timed path cannot write through it (std's flush would
block), and the first version kept its own 8 KiB buffer flushed at every
slice end. Codex showed what that changed with no limit reached: `f 2000
string writestring e (B) writestring` put B first; an `(A)` before an error
came after the report instead of before; an `EPIPE` arrived at the slice
end, outside the `stopped` that should catch it; a buffered `write` stopped
later had already lost its operands.

So the timed path now keeps std's buffer itself (`timed_write::LINE`):
process-wide like std's, 1024 bytes, with `LineWriterShim::write_all` and
`BufWriter::write_all`/`flush_buf` rules copied exactly (a write with no
newline is buffered while it is smaller than the room, flushes first when
it is bigger, goes straight through at 1024 bytes or more; a write with a
newline sends through the last one; `flush_buf` keeps what it could not
write). The flush points were pinned on the base binary first (1023, 1024
and 1025 bytes then a stderr byte; 1023 + 1; 1023 + 1 + 1; 1000 + 100);
`a_timed_run_merges_stdout_and_stderr_as_an_unlimited_one_does` runs them
and every writer, quit, stop and errors under `2>&1`, from `-e`, a file and
the REPL, with and without `--max-time`, and compares the merged bytes.
Bytes leave at the same operator as without a limit, and a write error is
that operator's (`stopped` catches `ioerror` as before).

**Owned by one interpreter** (Codex round 2, R2). The buffer
(`timed_write::LINE`) is process-wide, but one interpreter owns it at a time:
the first to write while it is empty claims it (its `host_id`), and it is
free again once empty. Another interpreter's timed writes meanwhile go
straight to the descriptor (its own flush points, not std's: several
interpreters sharing one stdout interleave as they write), its `Drop` and its
plain path never touch the buffer, and only the owner does the end-of-run
wait. The buffer and the stderr writers' lock are taken with `try_lock` and
the run's deadline, never a blocking wait, so a short-budget interpreter
cannot wait out a longer one stuck on the pipe.
`tests/stdout_owner.rs` runs several interpreters in one process (a child
copy of the test binary, stdout and stderr one pipe): dropping a captured or
unrelated interpreter leaves the owner's partial line in place, and another
timed interpreter's line goes out whole without joining it.

**Sent, not moved** (Codex round 2, R1). The buffer is sent
(`timed_write::send_line`), with no deadline, where std's buffer would be
flushed: the CLI's `stdout_text` and `flush_stdout` (its own output, the REPL
prompt, its last flush; `pscat::hand_over_stdout`), the interactive window's
prompt, `--bands`' flush, the owner's plain-path writes and the owner's
`Drop`. A send that fails keeps the bytes not written, as `flush_buf` does,
and returns the error, so the CLI's last flush still sees a reader that went
away: `f (A) writestring { f (\n) writestring } stopped { e (caught)
writestring } if` with stdout closed exits 141 with and without a limit
(`a_caught_flush_failure_still_fails_the_final_flush`). (The first round-1
version moved the bytes into std's own buffer instead; std's `LineWriter`
does not keep a line whose direct write fails, and the error was dropped.)
Nothing is moved into std's buffer any more, so the timed path no longer
flushes it either (that flush had no deadline): an embedder's unflushed
`print!` from before a run comes after the program's output. The
parent-gone watch, which calls `process::exit` from another thread, writes
the buffer first without waiting on a lock (`drain_now`; the main thread may
be stuck in a write holding it), waiting for room at most one polling
interval.

**End of a run.** A run that ends (finished, error, `quit`, a limit, found
in the slice or at its start or end) with bytes in the buffer under a time
limit writes nothing then. It records the deadline the later send of
those bytes is held to (`timed_write::end_of_run`), and whether the run
finished. The bytes leave where std's buffer would be flushed, so every
order against stderr is the plain path's by construction: after an error
report, `--pstack-on-error`, `--profile`'s table, `--lint`'s verdict,
`pscat: wrote FILE`, a sweep frame's failure
(`what_the_cli_prints_after_a_run_keeps_its_place_under_a_time_limit`).
Those flush points are the CLI's own writes (`stdout_text`,
`flush_stdout`, through `pscat::hand_over_stdout`), the owner's next plain
write, and a library user's `Drop`. The send goes through the timed writer
(no more than the descriptor takes, the deadline asked before the first
byte and whenever it waits), and past the deadline the rest is dropped.
Nothing predicts room. (Round 3 tried room checks: a full macOS socket was
called roomy, and so was a Linux pipe whose first page was partly read, and
the exit flush hung. Round 3's second try sent the line at the end of a
finished run, which put it before the `--profile` table; its `-e`/`--png`
checks used `print`, which flushes, so they tested nothing.)

- The CLI's interpreters (`Options::new_interp`: the main run, every sweep
  frame, every band worker) are marked with `leave_stdout_to_process`:
  their `Drop` gives the bytes up instead of sending them, as std's buffer
  outlives an interpreter, and the next frame appends to them or the CLI's
  next flush sends them. A sweep frame's stdout is the process's stdout,
  so it is held to the limit like any other.
- A drop of a finished run's bytes is the run's only sign of the limit.
  `send_line` notes it (`pscat::stdout_dropped_at_limit`), and `main` ends
  with `pscat: limitexceeded (time): ...` on stderr and exit 1, after its
  last flush, which is where the bytes would have left
  (`a_full_socket_at_the_end_of_a_run_does_not_outlast_the_limit`,
  `a_sweep_frames_stdout_is_held_to_the_limit_and_a_drop_is_reported`). A
  run that failed has reported its own error, so its dropped tail is not
  reported again. A send that fails (`EPIPE`) keeps the bytes for the
  CLI's last flush, so the exit is 141 as without a limit.
- A library user's interpreter sends them in its `Drop`, or at its next
  plain write, held to the deadline; past it they are dropped silently
  (`tests/stdout_owner.rs` pins the order across several runs on one
  `Interp`).

The deadline is the run's own, but never less than `END_GRACE` (50 ms) from
the start of the send. With none, a run stopped by its time limit would
drop its last partial line before the first byte even to a reader that is
taking it, where an unlimited run (and the base) delivers it
(`a_run_stopped_at_its_limit_still_sends_its_last_line_to_a_drained_pipe`).
It also means the CLI's work after a run (writing a large PNG) never
counts against a line a drained pipe would take at once. 50 ms is two
polling intervals and scheduling slack, enough for a reader that is
draining to take std's 1024 bytes, and short against any limit. With a
host interrupt flag and no time limit nothing is recorded: the bytes wait
for the exit flush as without the flag (the writes themselves still stop
when the flag is raised).

**State after a stop.** Bytes already written stay written; a stop in the
middle of a flush keeps the unwritten part in the buffer as `flush_buf`
does, and the end-of-run rule then drops it if the pipe stays full. The
operator's operands are restored by the central operand ledger as for any
error: `f <1023 bytes> writestring f 65 write 1 2 f 66 write` into a full
pipe stops in that last `write` with `Operand stack (4): 1 2 -file- 66`
(`a_stopped_write_restores_its_operands`). A single buffered byte that does
not flush cannot be stopped inside its `write`: it writes nothing, as
without a limit, and the stop comes at the end of the run.

### What did not change

No time limit and no interrupt flag (`--max-time 0` included): the old
blocking `write_all` through std, plus one relaxed atomic load to know the
timed buffer is empty. Stdout captured (`capture_stdout`: in-process renders,
the wasm playground): unchanged. Windows: the old write (no `poll` on pipes
there; the module and its tests are `cfg(unix)`).

The MCP server runs the CLI as a child for its renders, with stdout and
stderr both pipes it drains on its own threads and always with a time
limit, so that child's program output takes the timed path on both
descriptors. That is why the merged order has to be exactly the plain
path's.

### Limits of the guarantee

- **One writer.** The room is read, then written; another process writing
  into the same pipe in between can fill it, and the write then blocks as it
  always did. On macOS a full pipe below 64 KiB is grown by a blocking write,
  which sleeps if the kernel cannot grow it (the system is out of pipe
  memory).
- **The CLI's own writes after the run** (the error report on stderr,
  `--pstack-on-error`, `pscat: wrote FILE`) are plain blocking writes: a run
  stopped with stderr a full, undrained pipe ends its program, but the
  process then waits on that report. The `%stderr` test uses a slow reader
  for that reason. The resource notes in `src/paths.rs`
  (`PSCAT_TRACE_RESOURCES`, a resource too large) and `--bands`' own
  messages are `eprintln!` too. There is no page-to-stdout output
  (`--png`/`--svg`/`--pdf` take a file name).
- A terminal stopped with XOFF (Ctrl-S), a named FIFO on macOS and a
  datagram socket block as before.
- Several interpreters in one process writing to one stdout: only the
  owner of the line buffer keeps std's flush points; the others write
  straight through. Another interpreter counts as a second writer: it can
  fill the room between the owner's look and its write, and the owner's send
  waits for the reader within its deadline. The timed path does not hold
  std's stdout lock, so another thread's line can land between the pieces of
  a long timed write. An owner whose limits are taken off between runs sends
  its partial line at its next (plain) write, earlier than std would; an
  owner dropped after a failed send gives up the buffer and leaves the bytes
  for the next writer and the CLI's last flush.
- The deferred send writes what the timed writer can tell fits.
  On a macOS stream socket that is `poll`'s word, which calls a socket with
  less than 2048 bytes free not writable: a finished run whose socket is
  nearly full and whose reader reads only after the exit now drops the
  line and ends with `limitexceeded (time)`, where an unlimited run's exit
  flush would fit it. It is the timed path's rule mid-run too. A Linux pipe
  (`pwritev2(RWF_NOWAIT)`) and a macOS pipe (`fstat`) are exact
  (`a_nearly_full_pipe_at_the_end_of_a_run_still_takes_the_line`).
- Bytes still buffered after a timed run are sent at the CLI's flush points
  and in a library owner's `Drop` within the recorded deadline. An owner
  that never ran under a time limit (an interrupt flag alone) sends them as
  std's flush does, waiting for the reader.
- The limit for a finished run is reported at the exit, not at the
  operator: the run itself ended without error. And once the send has
  dropped the bytes, the CLI's own plain stdout writes that follow (a
  sweep's progress line, the REPL prompt) still block on a pipe that stays
  full, as they always did.
- No test drives a panic: the CLI has no trigger for one. The mechanism is
  `Drop for Interp` running while it unwinds, with a lock that tolerates
  poisoning.

### Cost

Child CPU, min of 7 interleaved with a base-binary control, macOS, release
(a guard, not a goal; not tuned). The table is in PR #1465.
