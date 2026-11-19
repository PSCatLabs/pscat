## Binary sequence decoding and the limits (issue #1123, 2026-11-19)

Question from PR #1103 (#733): does decoding a large binary object sequence notice
`--max-time`, MCP `cancel_render` or an interrupt, or does it run to the end first?

**Measured** (release build, a Mac under heavy load, `pscat --headless FILE` where FILE is
`(seq) run count =`; every sequence is the largest the format allows, `MAX_BYTES` = 64 MiB):

| sequence | no limit | `--max-time 0.05` |
|---|---|---|
| 127 arrays sharing one block of 65535 integers (8.3M objects) | 0.36-0.49 s | stopped at 0.37 s |
| 69 arrays of 4.5M distinct 6-byte names (interned) | 0.96 s | stopped at 1.08 s |
| one `mark` and 64 MiB of padding (reading only) | 0.16 s | stopped at 0.16 s |

So the decode ran to its end before the limit was looked at: a miss of up to about a second
(more on a slow machine), and the error was blamed on the next operator (`=`), not the
scanner. Bounded, since a sequence decodes to at most one object per 8 bytes and strings
copy at most the body's size, but the default `Limits::untrusted()` clock is 55 s and the MCP
tool timeouts are a few seconds, so it was worth closing.

**Fix.** `binobj::decode` takes the interpreter's `DecodeBudget` (the one decode filters
charge, #1187) and charges it 8 bytes a slot and one byte a character. The budget looks at the
step cap every KiB and at the clock and heap every 64 KiB, so a decode stops within about
8192 slots (microseconds) of a limit, and its work counts as steps like a filter's. Both
callers pass it: `Interp::decode_sequence` (the scanner, `run`, `token`, `cvx exec` strings)
and the free `decode_sequence` used inside a `{ ... }` body (`scan_procedure`, via
`Lexer::decode_budget`). The fuzz entry passes `None`.

**Not changed.** The lexer's byte-at-a-time read of the sequence from a plain file (0.16 s for
64 MiB) does not look at the clock; a source that is a decode filter already charges the budget
there. 0.16 s is not worth a per-byte check.

**Cancellation.** Nothing to do in the decode. A render is cancelled by killing its process:
`run_child` in `src/mcp_server.rs` kills the child when the call's cancel flag is set, and the
CLI is stopped by a signal. There is no in-process interrupt flag to poll (`AtomicBool`s in
`bands.rs` only stop band workers after an internal failure).

**Tests.** A unit test in `src/binobj.rs` charges a `DecodeBudget` directly (step cap, a
deadline already past, no budget). `tests/limits.rs` `binary_sequences_1123` runs a 5 MiB
sequence file (524280 objects, eight top-level arrays) through `run` under a step cap and a
1 ms clock; without the charge the program would run to its end, because the eight arrays are
only pushed. Neither test measures wall time.
