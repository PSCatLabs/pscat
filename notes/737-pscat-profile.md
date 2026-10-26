## `pscat --profile`: per-procedure call counts and time (issue #737, 2026-10-25)

Reported by fractal_clouds, which wrapped chosen procedures in hand-written
counters (inclusive time only, whole milliseconds). `--profile` does it for
every procedure bound to a name, on stderr, after a headless run.

**Where the time is taken.** `execute_resolved(obj, via)` knows when a name
resolves to a procedure body (`via` is that name; `exec` and the conditionals
pass `NO_NAME`). With a profiler on, it calls `prof_call`, which pushes a
`Frame::ProfReturn` and then the body's `Proc` frame, and opens an entry in
`Profiler::active`. Markers and open entries stay one to one, in the same order.

**Why a marker frame.** `Frame::Proc` is popped the moment it yields its last
element (a tail call costs no frame), so the frame's pop is *before* the last
element runs. Closing the call there would hand the tail operator's time, often
the expensive `fill` or `for`, to the parent, and drop a tail-called procedure
out of its caller's inclusive time. Watching `estack.len()` between steps fails
too: a loop frame can pop and push back at the same depth inside one step. The
marker sits below the body, so it is reached only when the body and its tail
call are done. `cleanup_unwound_frame` (used by `do_stop` and `unwind_all`) and
`exit_loop` close it when something unwinds past it, which covers `stop`,
`stopped`, errors caught or not, `exit` and `quit`.

**Tail recursion.** A marker per call would turn a 60,000-deep tail recursion
(constant exec-stack depth in a plain run) into `execstackoverflow`. The
markers contiguous on top of the exec stack are exactly the procedures in tail
position, so a call to a name already among them counts but plants no marker.
Cost: with mutual tail recursion (`ping` <-> `pong`) the chain is one open call
under the last marker planted, so the bodies of both count as `pong`'s
exclusive time (tested).

**Exec-stack depth.** Each open named call holds two slots (marker and body
frame) against one, so non-tail recursion overflows at about half the depth:
about 5,000 levels under `--profile` against about 10,000 plain
(`execstackoverflow`, and the report still prints). Making the push sites
subtract the markers would put a cost on the hot path; not done.

**Accounting.** Each open call has a start time and the inclusive time of its
children. On close: `exclusive += duration - children`, the parent's children
`+= duration`, and the name's inclusive time `+= duration` only when its open
count falls to zero (once per outermost frame, so recursion is not counted
twice). Exclusive times are disjoint slices of the timeline, so exclusive is
never more than inclusive; a leaf has them equal.

**Clock.** `Clock::mono_ns` in `clock.rs` (a `OnceLock<Instant>` zero point;
0 on wasm32, where `Instant` panics). Nothing in the browser build turns the
profiler on.

**Overhead** (release, macOS arm64, CPU seconds, min of 5-9 interleaved runs,
control = the same binary twice):
- off, vs origin/main: `fib 30` 0.42 s vs 0.42-0.43 s (+2%); `benches/regression`
  (11 runs): fib 27 +2.2%, defloop +2.4%, sierpinski -0.8%, fern +0.9%.
- on: `fib 30` (2.7 million calls) 0.43 -> 0.58 s, about 55 ns per call;
  `gallery/fern.ps` 0.200 -> 0.210 s.
- Renders are byte identical with and without the flag (fern, chalkboard
  lecture, field notes, golden bloom).

**How the off cost got to 2%.** The first version made `step_n_inner` generic
over a `PROFILE` const and dispatched in `step_n`; a plain `fib 30` ran 19%
slower, and so did a version that only added a second caller of
`execute_resolved` and `push_proc_frame` (the compiler stopped inlining them
into the hot loop). The shipped version adds one `Option::is_some` test inside
`execute_resolved`'s `Array` arm and keeps everything else in `#[cold]`,
`#[inline(never)]` functions.

**Decisions.**
- `--bands` refuses `--profile`. Each band runs the whole program, so merging
  would show N copies of every call and N times the time; a profile of a plain
  run answers the question. (`--profile` also refuses sweeps, `--spool`,
  `--interactive` and the window: no single headless run to report on.)
- A name bound to another executable name (an alias) is attributed to the name
  it was called through; a body run by `exec` or a conditional belongs to the
  procedure that ran it.
- `--profile-json` is one line of JSON on stderr (`procedures`, each with
  `name`, `calls`, `inclusive_ms`, `exclusive_ms`; `total_procedures`).
- `--profile-top N` (default 20, 0 = all).

**Not done.** No call-graph (caller/callee) view, no per-operator time, no
file output. Procedures with no name, such as `forall` bodies, are invisible.
