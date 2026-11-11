## Fused conditionals in procedures (issue #864, 2026-10-26)

Phase 2 of #863. A procedure literal followed by `if`, or two procedure
literals followed by `ifelse`, now run as one fused step when the operand
under the first literal is a boolean. The lead came from #863's profile:
`basin_and_range` spent 8% in `drop_in_place<Value>`, mostly `if`
dropping a procedure it did not run. The owner's bar: performance counts,
but never at the expense of correctness.

### Measured first: what runs

A throwaway build counted the executed element pairs and triples in
procedure bodies:

- `gallery/basin_and_range.ps` and `gallery/fern.ps` are not bound. 69%
  and 64% of their elements are executable names, so a fusion over
  operator objects alone would never touch them. That is why the fused
  path also takes the name `if` (or `ifelse`) when it resolves to the
  operator right then.
- The drykit boards (`chalkboard_lecture`, `crayon_drawing`) are bound.
  In `chalkboard_lecture`, `{ } if` is 1.5% of elements, the
  `{ } { } ifelse` triple 1.2%, and `/name exch def` 2.25%.

### Design

- **No body form.** Nothing is compiled, cached or remembered. When
  `proc_step` meets a procedure literal, it hands off to
  `Interp::proc_literal`. That code looks at the next one or two elements
  of the live array, under one borrow and within the view's bounds. Reading
  them together is the same as reading them a step apart: pushing a literal
  runs no code that could change the body or the dictionaries in between.
  So `put`, `putinterval`, `astore`, `copy`, an aliasing `getinterval` view
  and `restore` are all seen, with nothing to invalidate or journal.
- **Built when?** Never. It is a peephole at dispatch time.
- **Error fidelity by construction.** A run fuses only where the
  conditional cannot fail:
  - a boolean condition;
  - procedure literals as its operands;
  - room on the execution stack for the branch.

  Anything else runs the elements one at a time, so every error is
  raised by the unfused elements, with the same operand stack, `$error`
  and report. For an unbound name, `last_name` is set as running the name
  would set it.
- **Operator identity.** An element counts as `if` when it is an operator
  object registered under that name. The check is by pointer against the
  `&'static str` that systemdict's own object carries, then by text.
  - The first version compared function addresses with `fn_addr_eq`. Rust
    guarantees neither that a function has one address nor that two
    functions have two.
  - A trial `/name exch def` fusion showed the problem: it matched in
    the ci test binary but never once in the release binary.
- **Steps.** A fused run of n elements is charged n steps, through
  `Interp::slice_left`, and runs only when the slice has n steps left. So:
  - `step_n(1)` never fuses;
  - the stack after every `step_n(b)` equals the single-step trace;
  - `--max-steps` passes and fails at exactly the same count (all pinned).
- **Profiling.** `step_n_timed` (`--profile-ops`) never fuses, and does not
  time the literal, so operator counts and times are unchanged.
  `--profile` attributes the branch as before: a conditional's branch is
  never a named call.
- **Hot loop.** The literal reaches the plain loop as
  `ProcStep::Op(proc_literal_op)`, with no new arm. An extra arm in
  `step_n_inner` cost every element about 5 instructions, which ate the
  gain. Two other shapes made it worse:
  - a separate match arm, through register pressure;
  - an or-pattern with a `Lit` variant (about +7 per group).

  A const parameter gives `step_n_timed` its own `ProcStep::Lit`.

### Instructions retired

Each row is the instructions for one group. The script runs 5 copies of
the body per iteration of a bound 1M `for` loop, and subtracts the empty
loop. Instructions retired are deterministic.

| group | base (ebdd812) | branch |
|---|---:|---:|
| `1 pop` | 146 | 148 |
| `1.5 2.5 mul pop` | 324 | 322 |
| `1 2 lt pop` | 308 | 306 |
| `x y mul pop` | 520 | 526 |
| `false { 1 pop } if` | 456 | 414 |
| `true { 1 pop } if` | 693 | 633 |
| `-3 dup 0 lt { pop 0 } if pop` | 903 | 834 |
| `3 dup 0 lt { pop 0 } if pop` | 666 | 617 |
| `true { 1 } { 2 } ifelse pop` | 835 | 662 |
| unbound `false { 1 pop } if` | 618 | 518 |
| unbound `3 dup 0 lt { pop 0 } if pop` | 1308 | 1205 |
| unbound `true { 1 } { 2 } ifelse pop` | 1157 | 912 |
| `{ } pop` (literal, no conditional after) | 213 | 261 |

Code layout alone moves a row by about ±6, and a whole file by about
±1%, between builds that differ only in an unrelated arm.

Whole files, instructions retired:

| file | base | branch |
|---|---:|---:|
| `gallery/basin_and_range.ps` | 19.32G | 18.67G (−3.4%) |
| `gallery/fern.ps` | 3.61G | 3.54G (−1.9%) |
| `gallery/chalkboard_lecture.ps` | 81.8G | 79.8G (−2.4%) |
| `gallery/crayon_drawing.ps` | 104.9G | 102.3G (−2.5%) |

#863's microbenchmark (its issue body, `ns/op` per token, the best of 3
runs per binary, Apple M4, load about 4–8):

| body | base | branch |
|---|---:|---:|
| empty loop iteration (not subtracted) | 19.0 | 19.0 |
| `1.5 2.5 mul pop` | 6.0 | 5.5 |
| `1.5 dup mul pop` | 6.6 | 6.4 |
| `x y mul pop` | 10.0 | 9.9 |
| `/x 1.5 def` | 10.0 | 9.8 |
| `1 2 exch pop pop` | 5.6 | 5.0 |
| `true {1 pop} if` | 13.25 | 11.25 |
| smoothstep via def | 7.9 | 7.4 |
| smoothstep on the stack | 5.55 | 5.85 |

Only the `if` row moves beyond noise. CPU time on real files
(`scripts/bench_compare.sh`, min / median, with a base-binary control):

| file | base | branch | control |
|---|---|---|---|
| `gallery/basin_and_range.ps`, N=5 | 0.84 / 0.85 | 0.80 / 0.80 | 0.85 / 0.85 |
| `gallery/chalkboard_lecture.ps`, N=3 | 4.26 / 4.27 | 4.11 / 4.13 | 4.27 / 4.27 |

Identity: `scripts/identity_check.sh --base-bin/--head-bin` over every
tracked `examples/`, `gallery/` and `selftest/drivers/` file (136
files): all 177 page renders byte-identical. The two binaries' hashes
differ (no `--control`: a pure speed change has no head-only behaviour).

### Tried and dropped

- **`{ } if` alone** (the `if`-only commit): `basin_and_range` gained 3.6%,
  but every `{ } { } ifelse` paid a failed look-ahead (835 → 1013), and
  the drykit boards got 1% slower. It ships only together with
  `ifelse`.
- **`/name exch def`.** It is infallible (`def` with a name key cannot
  fail), and fused it went from 482 to 411 instructions. But to see the
  literal name, it had to leave `proc_step`'s inline push, and every other
  literal name paid +77 instructions (`/v 5 def` 405 → 482). In the drykit
  boards other literal names are about 2.5 times as common (7.9% of
  elements against 2.25%). Its semantics tests stay as pins.
- **Gating the literal on a boolean inside `proc_step`**, to keep the
  plain path inline: `1 pop` went from 147 to 220 through register
  pressure.

### What is left

- A procedure literal with no conditional after it (`{ } for`, `{ } def`,
  `{ } forall`) costs about 48 more instructions: one out-of-line call. It
  is followed by an operator that costs far more, but fusing
  `{ } repeat`/`for` into the loop frame, as #863 suggested, would win it
  back.
- Literal folding (`2 mul`, `0 lt`) and `dup mul` / `exch sub` were not
  tried. Their trigger is a number literal or an operator, and both sit on
  the hottest path; this work shows that a check there costs more than a
  pair saves. A precompiled form (with invalidation) would be the way to
  get at them.
- `eq` on two operators (`ops/logic.rs`) compares function addresses,
  with the same non-uniqueness caveat.
