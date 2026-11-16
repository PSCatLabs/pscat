# 1213: the step limit counts the slice it is in

Found in the Codex round-2 reviews of #1208 and #1217.

## The bug

Steps run in slices. The objects a slice ran were added to `steps_used` only
when the slice ended with more work to do (`after_slice`, `if more`). A program
that finished, or failed, inside its slice never charged them, and every look
at the limits from inside the slice (`limit_probe`, `poll_limits`, `poll_read`,
`poll_slow`, the decode filters) used `steps_used` as it stood when the slice
began. So:

- under `--max-steps 20`, a drain of 16,000 decoded spaces (15 steps) and a
  few objects ran clean, or ended in `undefinedresult` with `1 0 div`, though
  the program used more than 20 steps;
- a `flattenpath` preflight counted the steps left at the start of the slice,
  so it passed, replaced the path, and only then did the slice raise the limit
  (#1217 round 2).

## The fix

The step loop already keeps `slice_left` (the steps it has left, decremented
per step, charged by a fused run). A new field `slice_budget`, one store per
slice, makes `slice_budget - slice_left` the steps run so far. Nothing is
added to the hot loop.

- `Interp::steps_so_far()` = `steps_used` + those steps + the iterations `tick`
  has not charged. `remaining_budget` (so `limit_probe`), `poll_slow`,
  `poll_read` and `poll_limits_slow` use it.
- `after_slice` charges the slice's steps on every exit. A slice that ends in
  an ordinary error past the cap reports `StepsExceeded`, the cause, in its
  place. The two early `return Ok(false)` exits (quit, the pass that finds the
  program done) give their step back: no object ran.
- The count is now the same however the program is sliced
  (`the_count_does_not_depend_on_how_the_program_is_sliced`).
- A slice that ran its whole budget is still held to the cap by the next
  `limit_budget`, as before, not at its own end: a host that re-arms a cap per
  frame with a frame as big as the cap (the playground, #1210) must not meet
  its own cap in one frame.

## Pinned numbers that moved

`tests/dispatch_semantics.rs` pins the smallest cap a program passes. They
were measured on programs that fit one slice, where the objects were never
charged, so the figure was the iterations alone (and, for a program the cap
split into slices, a mix). They are now objects plus iterations, which is what
a long program was always charged:

| program | before | now |
|---|---|---|
| `5000 { } repeat` | 5000 | 5007 |
| `0 1 1 3000 { add } for pop` | 3007 | 6007 |
| the fused-run program | 756 | 806 |

Long programs (the gallery's 2.3 billion steps, the playground's per-frame
5,306) are counted as before.

## Left alone

`DecodeBudget` compares a drain to the steps left at the start of the slice. A
filter cannot read the interpreter's loop, and a per-step refresh costs the hot
loop. The overrun is under a slice and the end-of-slice check reports it; the
operator named in the report may then be the next one.
