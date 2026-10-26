## Per-token dispatch cost (issue #863, 2026-10-26)

The goal was to cut the flat 10–12 ns per token a bound procedure costs.
The owner's bar: performance counts, but not at the expense of
correctness or completeness. So every change kept error states, step
counts and renders exactly as they were.

### Profile on main (26d7c24)

The program was a `sample` (1 ms) of a 20M-iteration bound loop:

```
0.3 dup dup mul exch 2 mul 3 exch sub mul pop 1 2 exch pop pop 1.5 2.5 add pop
```

| function | share |
|---|---:|
| `step_n` (inlined `next_item`, fetch, dispatch) | 69.9% |
| `drop_in_place<Value>` | 7.8% |
| `mul` | 6.4% |
| `pop` | 3.1% |
| `Value::clone` | 2.5% |
| `sub`, `dup`, `add`, `exch` | 2.4%, 2.4%, 2.3%, 1.5% |

`/usr/bin/time -l` measured 77.0G instructions for that run, about 256
instructions per token. In the disassembly:

- `PsArray::peek` was not inlined.
- Each literal paid an out-of-line `Value::clone`, even for an integer.
- Each pop paid an out-of-line `drop_in_place<Value>`.
- Every element went through the `Action` enum, `sync_scan_line`, a
  40-byte `Item` return and `execute_element`.

On `gallery/basin_and_range.ps` the split was:

| function | share |
|---|---:|
| `step_n` | 38.7% |
| `execute_name` | 20.6% |
| `mul` | 4.0% |
| `Value::clone` | 3.9% |
| `drop_in_place<Value>` | 3.4% |
| `def` | 3.0% |

### What changed (one commit each, measured in instructions retired)

Instructions retired are deterministic, so they were the per-commit
measure on a machine other agents were loading. A group is one copy of
the body. The script runs 5 copies of it per iteration of a 1M bound
`for` loop, then subtracts the empty loop.

1. **`proc_step`.** Both step loops look at a `Frame::Proc` first.
   - The element is inspected under one borrow.
   - A literal is pushed in place (`Object::clone_inline` copies numbers
     without the call).
   - An operator or name comes back as a fn pointer or id.
   - `peek` is `inline(always)`.
   - The tail pop drops only the frame's body.

   `1 pop` went from 297 to 161 instructions a group. The 15-token
   run went from 77.0G to 50.5G.
2. **Operator fast paths that succeed or defer.**
   - `binary` (`add` `sub` `mul` `min` `max`), `div`, the relational
     operators, `eq` and `ne` look at the top two operands without
     popping (`top2_nums`). Two numbers are combined in place
     (`replace_top2`).
   - Anything else runs the old code on an untouched stack. A failure
     therefore leaves the same operands as before: `1 (a) add` leaves
     `1`, and `(a) 1 add` leaves nothing.
   - `pop` drops plain data inline (`Object::drop_inline`). `dup`
     copies numbers inline. `index` with an in-range integer replaces
     it in place.

   `1.5 2.5 mul pop` went from 407 to 321 instructions, and `1 2 lt pop`
   from 432 to 304.
3. **Name loads and `def`.** A name copies a number out of the dict
   inline. The `undefined` error is built in a cold function. `def`
   with no save open borrows the top dict in place, with no `Rc`
   clone. `x pop` went from 257 to 242 instructions, and `/x 1.5 def`
   from 423 to 401.
4. **`Item::Proc`.** `next_item` never runs a procedure element. When
   a loop iteration leaves a `Proc` on top, it returns `Item::Proc`
   and the loop calls `proc_step` in the same step. The step count is
   unchanged. `1 1 2 { pop } for` went from 1397 to 1270 instructions.
5. **`PsArray` drop.** The refcount check is `inline(always)`, and the
   last handle's teardown is a separate out-of-line function. The
   `for` group went from 1270 to 1199 instructions, and the call of an
   empty procedure from 216 to 198.

### Before and after, instructions

| group | main | branch |
|---|---:|---:|
| `1 pop` | 297 | 147 |
| `1.5 2.5 mul pop` | 675 | 325 |
| `1.5 2.5 add pop` | 676 | 324 |
| `1 2 lt pop` | 700 | 309 |
| `1.5 dup mul pop` | 692 | 356 |
| `1 2 exch pop pop` | 727 | 373 |
| `1 2 3 1 index pop pop pop pop` | 1389 | 696 |
| `x y mul pop` | 859 | 521 |
| `/x 1.5 def` | 626 | 406 |
| `true { 1 pop } if` | 1064 | 694 |
| empty `for` iteration (`{ pop }`) | 375 | 298 |
| 20M x the 15-token body | 77.0G | 40.6G |
| `fib 27` (bound) | 1.47G | 0.85G |

### Before and after, time

These are the issue's microbenchmark rows, each the best of 5 runs in one
process (`usertime`, Apple M4, load about 10). Each value is the per-token
cost in ns after subtracting the empty loop.

| body | main | branch |
|---|---:|---:|
| empty loop iteration (not subtracted) | 25.5 | 19.5 |
| `1.5 2.5 mul pop` | 11.5 | 5.75 |
| `1.5 2.5 add pop` | 12.0 | 5.63 |
| `1.5 dup mul pop` | 12.0 | 6.25 |
| `x y mul pop` | 14.5 | 9.75 |
| `/x 1.5 def` | 16.83 | 10.33 |
| `1 2 exch pop pop` | 9.6 | 5.1 |
| `1 2 3 1 index pop pop pop pop` | 9.75 | 4.75 |
| `true { 1 pop } if` | 19.0 | 13.5 |
| smoothstep via `def` | 12.83 | 7.42 |
| smoothstep on the stack | 11.15 | 5.9 |

These are real files, timed with `scripts/bench_compare.sh` (CPU seconds,
interleaved, with a base-binary control):

| file | main | branch | control |
|---|---|---|---|
| `gallery/basin_and_range.ps`, N=5 | 1.26 / 1.26 | 0.87 / 0.88 | 1.26 / 1.27 |
| `gallery/fern.ps`, N=3 | 0.19 / 0.20 | 0.15 / 0.16 | 0.19 / 0.27 |

Each cell is min / median. `basin_and_range` drops from 26.1G to 18.9G
instructions retired.

### Tried and rejected

- **Resolving a name under the dict borrow.** The idea: hand an operator
  back as its fn pointer, and push a literal in place, with no clone.
  It added about 4 instructions per name load and 32 per procedure
  call: `execute_name` grew a bigger frame and more unwind paths, and
  that cost more than the clone saved.
- **An inline `Array` (or `Boolean`) arm in `clone_inline`.** Procedure
  bodies met in a body, such as `if` and `for` bodies, are common
  literals. But the extra inline `Rc` increment in `proc_step` raised
  `1 pop` from 146 to 213 instructions a group, and every other row
  rose with it. Register pressure in the hot loop is the likely cause.
  The same arm used only for name lookups saved nothing.
- **Inline caches for names.** These were a measured non-goal (#330,
  Stage 11).
- **Shrinking `PsError`.** It is 32 bytes because two variants carry a
  `String`, and every operator returns it through memory. Boxing the
  payloads changes a public type, so it is left as a follow-up, not
  measured here.

### What is left

- `basin_and_range` on the branch: `step_n` 36%, `execute_name` 22%,
  `drop_in_place<Value>` 8%. That last share is mostly
  `ps_if` dropping a procedure when the condition is false (an `Rc`
  decrement). In the old shape, `{ pop 0 } if` costs a procedure push
  (`Rc` increment) and a drop. This is what #864's superinstructions
  (a fused compare-and-branch) would remove.
- A `for` iteration still costs about 225 instructions beyond its body:
  - the `Action` round trip;
  - an `Rc` clone of the body;
  - a 64-byte frame push and pop.

  Keeping the loop's `pc` in the loop frame would avoid the push and
  pop, but `exit`, `stop` and the tail pop all rely on the separate
  `Proc` frame, so that is a design change of its own.

### Correctness

- `tests/dispatch_semantics.rs` was written first and passes on the base
  tree. It pins:
  - the operand stack, `$error /errorname` and `/command` for failing
    arithmetic, comparison and stack operators. Each case runs
    unbound, bound and in tail position. Bound, an operator fetched as
    a fn pointer does not update the offending command.
  - NaN, `-0.0` and executable numbers;
  - the top-level report;
  - `step_n(1)` counts;
  - a `put` into a running bound procedure.
- The existing tests also pass, including `limits`, `profile` and
  `control`.
- Identity: `scripts/identity_check.sh --base-bin/--head-bin` over every
  tracked `examples/`, `gallery/` and `selftest/drivers/` file (136
  files). All 177 page renders are byte-identical. The run used no
  `--control`: a pure speed change has no behaviour that only the head
  binary shows. The two binaries' hashes differ.
