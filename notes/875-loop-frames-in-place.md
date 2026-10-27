## Loop bodies run in place (issue #875, 2026-10-26): measured and dropped

The idea came from #863 and #864. `repeat`, `loop`, `for` and `forall`
would run their body from the loop's own frame, so an iteration no longer
pays an `Rc` clone of the body and a frame push and pop. #863 put the cost
of a `for` iteration beyond its body at about 225 instructions.

**The measurements disprove that premise.** The clone and the push and pop
are only 20–35 instructions of an iteration. The rest is work any design
keeps:
- the step that finds the body finished;
- `next_item`'s dispatch;
- advancing the loop and `tick`;
- pushing the control value.

Both designs kept the semantics exact: every new pin below passed under
each of them. They were dropped on cost alone. No variant brought any of
the three real files down, and short loops got dearer in all of them. The
change was reverted. What ships is the tests, these notes and a HANDOFF
line.

### What is pinned (tests/dispatch_semantics.rs)

Written first, these pass on the tree before the change and under both
designs:
- **`exit` positions.** At the first, a middle and the last element of a
  body. Through a named procedure, called last and not last. Through
  `exec`. From an `if`/`ifelse` branch. In nested loops, including an inner
  loop that is the outer body's last element. Across `stopped`, as
  `invalidexit`.
- **`stop` and errors.** `stop` inside loops inside `stopped`. Errors at
  the first, middle and last element of every loop kind, caught and
  uncaught, bound and unbound, with the operand stack and `$error`'s
  `errorname` and `command`.
- **Steps.** `step_n(1)` counts for every loop kind, including empty
  bodies. Slice traces (`step_n(b)` ends where `b` single steps do). The
  smallest `--max-steps` each of eight loop programs passes.
- **Exec-stack depth.** The `execstackoverflow` depth for recursion
  through loop bodies, in tail and non-tail position, through a fused `if`,
  and with the depth shifted by one frame so the overflow lands on a
  different push. Pinned by the operand count.
- **Profiles.** `--profile` call counts with `exit` through a profiled
  procedure, and `--profile-ops` operator counts.
- **A body changed while it runs.** `put`, `putinterval`, a
  `getinterval` view, and a `restore` that puts back an element of the
  running body.

### Design A: the loop state in the procedure frame

`Frame::Proc { body, pc, looping: Option<Box<LoopState>> }` replaced the
four loop frames:
- `pc` walked the body;
- at the end, `next_item` advanced the boxed state and set `pc` back to 0;
- a loop frame was never popped at its tail.

The old shape had two frames mid-body. A counter of loop frames part way
through their body kept the `execstackoverflow` depth exact:
- raised when an iteration starts;
- lowered at the body's tail, by `exit_loop` and by
  `cleanup_unwound_frame`;
- checked against a scan after every slice in debug builds.

Its costs:
- the `Box` (one allocation through the tracking allocator) cost 300–430
  instructions per loop entry;
- every procedure element cost more.

### Design C': the body's frame reused across iterations

The loop frames stayed as they were. A loop's first iteration pushed its
body's `Proc` frame with a `looping` flag. At the body's tail that frame
stayed, finished (`pc == body.len()`), where a plain frame is popped. The
next iteration set `pc` back to 0. Two pieces kept the depth exact:
- a counter of finished loop bodies, subtracted from the frame count in
  the depth check (the old shape had popped them);
- `exit_loop`, `cleanup_unwound_frame` and the fused conditional's tail
  each adjusted the counter.

Nothing was allocated, and plain procedure frames kept their layout.

### Instructions retired

Each group row is the instructions for one group. The script runs 5 copies
per iteration of a bound 1M `for` loop, less the empty loop. The iteration
rows are per iteration, less start-up. Code layout alone moves a row by
about ±6 (#864).

| row | base (82c89a2) | A | C' variant | C' flag | C' flag, fast restart |
|---|---:|---:|---:|---:|---:|
| `for` iteration, `{ pop }` | 259 | 200 | 243 | 239 | 227 |
| `repeat` iteration, `{ 1 pop }` | 257 | 227 | 281 | 271 | 272 |
| `forall` iteration, `{ pop }` | 298 | 237 | 274 | 270 | 261 |
| `1 pop` | 148 | 163 | 167 | 154 | 170 |
| `1.5 2.5 mul pop` | 316 | 344 | 352 | 326 | 359 |
| `x y mul pop` | 520 | 554 | 560 | 530 | 557 |
| `true { 1 pop } if` | 633 | 680 | 674 | 645 | 676 |
| `0 1 2 { pop } for` (a short loop) | 1507 | 1816 | 1650 | 1575 | 1754 |
| `1 { 1 pop } repeat` | 748 | 1176 | 937 | 892 | 1079 |
| `3 { } repeat` | 621 | 1067 | 850 | 819 | 951 |
| `fib 27` (no loops) | 737.8M | 788.9M | 782.8M | 753.2M | 798.1M |

Whole files, instructions retired:

| file | base | A | C' flag | C' flag, fast restart |
|---|---:|---:|---:|---:|
| `gallery/basin_and_range.ps` | 18.17G | 18.85G | 18.31G | 18.77G |
| `gallery/fern.ps` | 3.34G | 3.41G | 3.36G | 3.40G |
| `gallery/chalkboard_lecture.ps` | 78.62G | — | 79.53G | 83.32G |

The best variant, C' with the flag, is +0.2% to +1.2% on the real files.
That sits near the ±1% layout noise floor. What decides against it:
- no variant made a real file faster;
- every variant made short loops dearer;
- `fib`, which runs no loop, rose 2–8%.

### Dead ends

- **A second frame variant** (`LoopProc`). The compiler moved `Frame`'s
  tag into a niche, so telling `Proc` from `LoopProc` cost every procedure
  step a three-instruction decode.
- **`#[repr(u8)]` to force an explicit tag.** `Frame` grew from 64 to 72
  bytes, because `Forall` had been holding the tag in `ForallSrc`'s niche.
- **`next_item` out of line.** Elements got a little cheaper, but every
  iteration paid the call (282 instructions).
- **The restart taken straight from `proc_step`**, skipping `next_item`.
  Iterations got cheaper (227), but the larger `proc_step` cost every
  element 10–20 instructions.
- **Inlining the iteration code into `next_item`.** No better.

### What is left

- **Iterations.** The only way to cheapen them is a separate path through
  the step loop that skips `next_item`. #863 and #864 showed the hot loop
  pays for any such path in register pressure.
- **Fusing `{ } repeat`/`for`/`loop`/`forall` in `proc_literal`.** This is
  #864's suggestion, and it does not need bodies run in place: it would
  build today's loop frames directly, saving per loop entry, not per
  iteration. Its ceiling should be bounded first: count the loop entries
  in the real files, times the 50–100 instructions saved. Then set that
  against the look-ahead every other procedure literal would pay.
