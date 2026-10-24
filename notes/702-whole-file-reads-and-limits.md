## Whole-file reads under execution limits (issue #702, 2026-10-23)

**Problem.** `(/dev/zero) (r) file` and `file` on a FIFO read the whole source
inside one operator (`paths::open` -> `std::fs::read`), so the step, time and
memory limits from #604 never ran: `/dev/zero` reached 1 GB in seconds, and a
FIFO nobody writes to blocked in `open`. Only the MCP server's `--timeout` kill
stopped it, and the CLI had nothing.

**What shipped**
- With any limit set, `read_resource` (`src/ops/file.rs`, used by `file` and
  `run`) reads through `file::read_path_limited` and
  `paths::open_with_reader`; with none set it is the old `std::fs::read`.
- A regular file is read on the calling thread in 64 KiB chunks. Its length is
  checked against the memory cap first (`check_alloc`) and the buffer is sized
  once from it (`try_reserve_exact`), so no doubling can be refused halfway
  for a file that passed the check; a file that grows past its reported size
  (`/proc`) has the next growth announced to the check before it is made.
- Anything else (device, FIFO, socket, terminal) is read on a helper thread
  through a 2-slot channel, so the open and the read may block while the main
  thread wakes every 10 ms to look at the limits. A limit abandons the helper
  (it exits on its next failed send, or stays blocked until the process ends).
- `Interp::poll_read` charges one step per KiB read (so `--max-steps` bounds a
  read) and then runs the usual time and memory checks. Any limit error is the
  usual uncatchable `PsError` (`StepsExceeded`/`TimeExceeded`/`MemoryExceeded`).
- A limit stops the resolver's search: `read_resource_limited` remembers the
  error and returns it even if the search then found the embedded copy of a
  `lib/` name.

**Decisions**
- Not "refuse non-regular files": the issue's acceptance check wants a limit
  error from `/dev/zero`, and refusing would break `/dev/stdin` pipes for no
  gain once the read is interruptible.
- No byte cap with no limits set: the CLI default stays a trusting `fs::read`.
- A step cap alone does not free a FIFO with no writer (no bytes, no steps);
  a time cap does. Documented in SECURITY.md.
- The MCP timeout kill is now the backstop for anything else stuck in one
  operator; no test exercises it any more (the FIFO test became a
  `limit_exceeded` test).

**Not done**: `pscat-mcp`/CLI flags unchanged; the other readers of files
(`subcmd::open`, font catalog loading) are not program-driven whole-file reads
of arbitrary paths and are untouched.
