# #1081: `[stack-leak]` says where the oldest leftover item came from

Before: `20 item(s) left on the operand stack at the end of the program: ... (top 5 of 20 shown, topmost first)`,
which says something leaked but not where. Now the same text, then
`; the oldest, <item>, was left when the program reached line N` (` (file)` for a `run` file).
The oldest item is the first one leaked; the ones above it are often consequences (a missing
`def` shifts every later operand). A stack longer than the five shown names its bottom item
even without the trace (`; the oldest is <item>`), which is the issue's second ask.

## How

`Interp::trace_stack_leaks()` (hidden, on under `--lint` via `Options::new_interp`, so also
`--bands` and every `pscat-mcp` render) makes `sync_scan_line` record the operand depth each
time a source token (main program or `run` file) is about to run. At that boundary the stack
holds the result of every earlier token (a loop or procedure call has finished), so growth
since the last sample belongs to the previous token, whose line `last_line` still holds: it
is read before the new token overwrites it. The program's last token has no later sample, so
the lint credits any growth since the last one to `last_line` at the end.

`lint::StackTrace` keeps runs (`start depth -> line`), not one entry per item. A run is dropped
when the depth falls back to its start. `clear`, `aload` of 10 000 items or a loop pushing a
million values cost a few entries (test: `a_big_aload_keeps_the_trace_small`).

## What it says, and does not

The line is the one the scanner had reached, like #1109's. A leak made inside a loop or a
procedure is credited to the line of the token that ran it: the closing `} for` for a loop (the
whole `{ ... }` is one token), the call site for a procedure (not the `def`). That narrows a 150-line program to one statement. Without the trace (a library caller of `lint::check`) the message is the old one plus
the bottom item when the stack is deeper than five.

## Cost

See the PR for the measurement: a plain render pays nothing (one `Option` check per scanned
token); `--lint` pays one `Vec` length read and a compare per token.
