# #1366: `[stack-leak]` names the leaking element's own line

Follow-up to #1081 (`notes/1081-stack-leak-location.md`) and #1338, built on the
source-position table of #1405 (`notes/1405-source-positions.md`).

Before: a leak made inside a loop was credited to the closing `} for`, a leak
from a procedure to its call site. Now the line is the one the leaking element is
written on:

```
0 1 9 {            % was: line 5 (the closing `} for`)
  pop
  /gq 1            % now: line 3
} for
```

The wording of the finding, its check name (`stack-leak`) and `lint::LintFinding` are
unchanged, so `pscat-mcp` and the tests that match on `was left when the program reached
line N` still match; only N moves. No public item changed: `Interp::trace_stack_leaks`
is `#[doc(hidden)]` and keeps its signature, and nothing in `pscat::host` reads this
finding (the host's lint check is `[utf8-in-byte-font]`, #1446), so psanim's findings
are as before.

## How

The old trace sampled the operand depth only when the scanner was about to read a
token, so everything a token ran (a loop, a call) was one lump. Two things now:

1. **`step_n_timed`**, the existing copy of the step loop that `--profile-ops` uses, now
   also runs while `--lint` is tracing (`Interp::instrumented`, one flag, tells `step_n`;
   with a profile's timing on too, the loop times operators as before). Before each
   step, `note_stack_step` makes constant work: it notes the lowest the operand
   depth has been since the last token boundary (`low`), and, when an item has
   appeared at that low point, which procedure element pushed it. The element is the one the previous
   step started, kept as a `storage_site` (the body's address and the slot: no
   `Rc` held, no lookup). For a step that starts a loop iteration, the element is
   the first of the loop body, a guess the next sample confirms against the top
   frame (the same step runs that element); when the loop was in fact over, the
   element of the procedure now on top before its `pc` is used instead, which is the
   parent's element unless that was its last (then its frame is gone, and the call is named).
2. **The token boundary** (`StackTrace::note`, as before) drops the runs above `low`
   (they were consumed, which also replaces #1081's bottom-item guess) and pushes one
   run for what is left above it, whose origin is the token's line plus that
   element's site.

The element's site becomes a line at the boundary, in the one place that can show it is
still the right one (next section). The runs keep plain numbers (a line, a source name),
never an address, so nothing in them can name a body that is gone.

## Round 1 of the review (Codex, PR #1474): when a credit is kept

The first version trusted a credit for as long as the item stayed on the stack. A stack item
can be consumed and replaced inside one operator, which the depth does not show, and an
address can be freed and given to another body. Rule now: when the tracker cannot prove a slot
still holds the value it credited, it names the token's line (what #1081 named), never an own
line of a producer whose value is gone.

- **Witness.** When an item first appears at the low point its `Witness` (kind and scalar
  value; a composite by storage and extent) is kept with the element. At the boundary the
  element's line is used only if the item at that slot is still that. `]`, `>>`, `exch`,
  `roll`, `restore` and an arithmetic result over the pushed value all change it, so they
  fall back (`[ 77 ]` and `<< /a 77 >>` name line 3 again, as on the base). The bottom
  item's witness is checked at every boundary, as #1081 did: a replaced bottom item drops all
  the runs and credits the token.
- **Generation.** `BodyLines` counts every entry added or dropped (`register`, `forget`, a
  sweep, `begin_program`). A site carries the count it was taken at and is resolved only if it
  is unchanged at the boundary, which runs before the next token is scanned. A swept body's
  address given to another body is therefore never answered for (test
  `a_site_taken_before_the_table_changed_is_not_resolved` controls it with no allocator, and
  `a_swept_body_is_never_named_by_a_later_one` is the 32766-body program).
- **Programs.** `begin_source` calls `StackTrace::begin_program`: the runs of an earlier
  program name no line (an operand a host left on the stack is of unknown origin, not a line
  of the next text).
- The 1024 runs are allocated when tracing starts, so the memory cap sees them then and a run
  never grows the vector; the report no longer clones the trace.

## When the element's own line is named

Only when it was written in the text the scanner was reading when it left the item: the
main program, or the `run` file of that name. Otherwise the line of the token that ran it
is named, as before:

- a library procedure that returns a value the program does not pop: its own line in
  `lib/artkit.ps` is no use to its caller, the call into it is;
- a prelude the host named and ran in an earlier program (`set_main_file_name`, psanim's
  pattern, #1405): a line of another text;
- an element with no line (a procedure built at run time, an executable string).

The intermediate procedures between a library body and the program's token are not
walked: that would be a walk of the execution stack per sample. A call in tail position
has dropped its caller's frame anyway.

## What it is, and is not

- The oldest item is credited to the element that pushed the item at the low point of
  the token's run, while that item is what the element pushed. An accumulator (`7` then
  `2 mul`) or a swapped item names the token, not the element. A loop's index that the
  body does not consume is the loop's own push, made in the same step that runs the body's
  first element, so it is credited to that element's line (`0 1 9 {` / `1 pop` / `} for` names
  line 2); a loop with an empty body has no element and names the `} for` token (an own line
  of 0, what an empty body's table entry holds, is treated as unknown).
- A body of one element is named through the guess above only when the loop's frame is
  back on top with as many frames as before.
- A leak in a loop that `exch`es the index away, as in `/gq exch pop`, names the `} for` as
  before: the tracker cannot show which element's value the first item is.

## Bounds

- Per step: a depth comparison, and a handful of stores. Nothing is looked up or
  allocated, so the work is charged to the step it is in (`--max-steps`,
  `--max-time` and `--max-memory` see the same steps as before: the instrumented loop does
  not fuse `{ } if`, but a fused run is charged one step per element, so the count is the
  same, test `tracing_the_stack_changes_no_step_count_or_result`).
- The runs are at most 1024 (`MAX_RUNS`). Before, a program that leaked from a million
  different lines held a million runs; now the newest items past the cap share the last
  run's origin. Test `a_program_that_leaks_from_every_line_keeps_the_trace_bounded`.
- At a boundary that credits an element: one hash lookup and one binary search in one body's
  lines.

## Cost

`--release` binaries, base = `origin/feature/0.8.0` at `58bb0d24`, head = this branch,
`/usr/bin/time -l` instructions retired (the figure is stable; wall time is not):

| program | mode | base | head |
| --- | --- | --- | --- |
| 13.5M-token flat file (`1 2 add pop 3 4 mul pop` x 1.5M) | plain | 17 864M | 17 865M |
| same | `--lint` | 19 580M | 21 344M (+9.0%) |
| `0 1 3000000 { /x exch def } for` | plain | 3 523M | 3 525M |
| same | `--lint` | 3 727M | 4 148M (+11%) |
| `/f { 1 pop } def 0 1 10000000 { pop f } for` | plain | 12 508M | 12 508M |
| same | `--lint` | 12 514M | 14 500M (+16%) |
| `gallery/alpine_lake.ps` (a heavy, loop-bound render) | plain | 467 582M | 467 001M |
| same | `--lint` | 483 453M | 572 250M (+18%) |

Before the fixes of round 1 the same rows were +4%, +11%, +16% and +21% (against the
previous base): the witness taken at every token boundary is what the flat file pays, a
comparison of three words.

The plain loop is untouched: one `bool` test per slice replaces the profile test that was
there. A lint run pays the figures above: about two thirds of the alpine figure is the
instrumented loop not fusing `{ } if` (what `--profile-ops` needs), the rest the per-step
sample. Three other shapes were tried and dropped: a lookup of the element's line on every
push (+50% on the loop rows), a third copy of the plain loop with the sample in it (lint +15%,
but the plain loop then cost +2% in instructions on `alpine_lake` and the loop rows, as the
compiler's inlining of the shared helpers moved), and the first version of the sample (+86%).

## Not done

- Intermediate user procedures between a library body and the program's token are not
  named (above).
- A leak made by a procedure that `exec` builds at run time has no line of its own.
