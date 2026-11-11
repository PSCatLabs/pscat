## `--profile` follow-ups: call graph, JSON to a file, per-operator time (issue #799, 2026-10-26)

Three additions to `pscat --profile` (#737). Each is its own flag, and each
implies `--profile`.

### `--profile-graph`: caller -> callee

`Profiler::enter` records an edge from the innermost open call (or a
`<top level>` root) to the callee: calls, and inclusive time. The edge has an
open-depth like a name does, so a recursive edge (`fib -> fib`) adds its time
once per outermost frame; without that the self edge was about 14x the run on
`fib 15`. A tail call back into a name already among the markers
(`Profiler::count_only`) adds a call to its edge and no time, because its time
is part of the call it continues. That keeps the invariant the tests check:
for every callee, the calls over its incoming edges (the root edge included)
equal its row's `calls`, through tail recursion, mutual recursion and
unwinding.

Edge work is behind `Profiler::graph`, so plain `--profile` is unchanged
(`Active::edge` is `usize::MAX` and the edge map is never touched). Text view:
a second table; JSON: `edges` (`caller` null at the top level) and
`total_edges`. `--profile-top` limits the edges too.

### `--profile-json PATH`

The path is optional, and a program file can follow the flag
(`--profile-json prog.ps` works today), so the next argument is taken as the
path only if it does not start with `-` and ends in `.json`. There is no
`=` form (it would have been the first in the CLI). Any other file name is
still a program.

A path needs `--allow-write DIR`. `Interp::writable_path` is called twice: right
after the limits are applied, so a path outside the roots (or no roots at all)
fails before the program runs, and again just before writing, so what the
program did to the directory in between counts. A failed write is a failed run
(exit 1, message on stderr). With a path, stderr gets no report.

### `--profile-ops`: time per operator

Operators reach the loop as bare `fn` pointers (`Item::Op`), with no name and no
room to carry one without growing `Item` on the hot path. So the report maps
the function address to its systemdict name(s), built once when the flag turns
on. `step_n` checks once per slice whether operators are timed and, if so,
runs `step_n_timed`, a copy of `step_n_inner` that wraps `Item::Op` and
name-to-operator calls in two clock reads. The plain loop has no per-step test.

**The flag-off cost was the question** (issue: keep it within ~2% on `fib`).
Measured with `scripts/bench_compare.sh`, release, macOS arm64, CPU seconds, min
of 11-15 interleaved runs, the control being the base binary again:

| flag off, vs origin/main (77b40d7) | base | head |
|---|---|---|
| fib 30 | 0.43 | 0.39-0.40 |
| defloop 2M | 0.15 | 0.14 |
| gallery/fern.ps | 0.20 | 0.19 |

With the loop copy added and nothing else, `fib` was 20% slower (0.59 -> 0.71):
the second call site made the compiler stop inlining `next_item` and the
`execute_*` helpers into the hot loop, as in #737. Marking `next_item`,
`execute_element`, `execute_resolved`, `push_proc_frame` and `recover`
`#[inline(always)]` restored it (the table above is that build, which is not
slower than main, run to run; treat the improvement as layout luck, not a
feature).

Flag-on, `fib 30` (CPU s; main 0.43): `--profile` 0.55, `--profile-graph`
0.59, `--profile-ops` 1.28. `gallery/fern.ps` (0.20): 0.20, 0.21, 0.40.

**What the operator time means.**
- It is the operator's own code. `for`, `repeat`, `loop`, `forall`, `if`,
  `ifelse` only push a frame; their bodies are timed as the operators they run.
- `show`/`ashow`/`image`/`colorimage` do most of their work in `Frame::Show` /
  `Frame::Image` steps inside `next_item`, which the timer does not see, so
  they read cheap. `fill`, `stroke`, `arc` and the arithmetic are synchronous.
- Each reading includes the two clock reads (about 20 ns): a cheap operator's
  per-call figure is mostly that. The report prints calls and ns per call for
  that reason.
- An operator is labelled by its systemdict name; if two names share one
  function the label is `a/b`. A name that resolves to another name that is an
  operator (an alias chain) is not timed.
- `--profile-ops` also inflates the procedure table (two clock reads per
  operator inside every procedure), which is why it is not on by default with
  `--profile`.

### Not done

- Per-edge exclusive time; operator time attributed to the procedure that ran
  it (a "which procedure called `fill`" table).
- Timing the deferred work of `show` and `image`.
