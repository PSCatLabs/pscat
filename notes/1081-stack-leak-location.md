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

## Bottom-item witness

A token can consume the whole stack and leave another (`0 1 9 { ... } for` takes its three
operands and leaves what the body leaked), which a depth sampled between tokens sees only as
growth. Without a fix the first run (the `0 1 9` operands, on line 1) would survive and be
named. So the trace keeps a witness of the bottom item (kind and scalar value) and, when it
has changed, forgets the old runs and credits the whole stack to the token that ran. A guess. It is wrong
when the bottom item is replaced by an equal scalar (the old line is then still the statement
that began it), and for a composite (array, string, dict) it compares the kind only, so one
replaced by another of its kind within a single token keeps the old line. A bottom item
updated in place (an accumulator: `0` on line 1, `1 add` on later lines) is credited to the
last token that changed it. Reading `ostack` at the end of the program goes through the same
path, so the last token is credited too.

## Cost

Head `--profile ci` binary, 13.5 million tokens in a flat file (`1 2 add pop 3 4 mul pop`
repeated 1.5 million times), user CPU, three runs each: 0.86, 0.88, 0.85 s plain; 0.82,
0.82, 0.82 s with `--lint` (which also runs the operator-shadow trace). So `--lint` adds no
measurable cost to a token-heavy run. That compares head with head: a plain render now also
pays one `Option` check per scanned token (`sync_scan_line`), which was not timed against a
base binary. With tracing on, the per-token work is a slice-length read, a compare and, at a
new line, a small push.
