## CPU-time render-budget tests (issue #329, 2026-10-13)

`a_crayon_scene_renders_within_the_ci_budget` failed once at 2.06 s against
its 2 s budget with the machine's load average at 11. The budget tests
measured wall time, so every slice the scheduler gave another process
counted against them. They now measure CPU time.

**What changed.**
- `tests/common/mod.rs` gained `thread_cpu_time()`, `cpu_time_of(f)`,
  `within_cpu_budget(what, budget, tries, f)` and `scale_budget(d)`.
  `within_cpu_budget` runs the closure up to `tries` times, stops at the
  first run under budget, prints every try, and asserts the best one.
- Converted: the four DRYMEDIA 6.2 CI-budget tests (`drycrayon`,
  `drycharcoal`, `drychalk`, `drykit`; 2 s, best of 3), six `paintkit`
  hang-guard tests (`pkwash` x2, `pkfan` x4; 2 s, 0.9 s and 5 s bounds,
  best of 3), and the timer inside `artkit`'s #110 walkpath test (already a
  self-relative best-of-3; only its clock changed, and its bound now honours
  the scale variable). `drykit`'s per-case print loop asserts nothing, and
  now prints CPU time too. `grep -rn Instant tests/` finds only the
  non-unix fallback in `tests/common/mod.rs`.
- `Cargo.toml`: `[target.'cfg(unix)'.dev-dependencies] libc = "0.2"`.
  `libc` was already in `Cargo.lock` through winit and friends, so nothing
  new is compiled, only one line added to the lock.
- `tests/cpu_budget_helper.rs` tests the helper itself: a 300 ms sleep reads
  as under 100 ms of CPU, a budget of zero fails, a slow first try is
  forgiven.
- AGENTS.md documents the convention.

**Why thread CPU time, not child rusage.** The brief suggested
`getrusage(RUSAGE_CHILDREN)` around a spawned `pscat`. These tests do not
spawn the binary: they build an `Interp` on the test thread. Children
rusage would read zero. `RUSAGE_SELF` would count the other tests of the
same binary, which cargo runs as parallel threads of one process. So the
clock is `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`, available on Linux and
on macOS 10.12+ (the CI runners). The interpreter spawns no worker threads
outside the live window, so the test thread's CPU time is the work under
test. A non-unix build falls back to wall time.

**Caveats, measured not assumed.** CPU time is not perfectly load-immune:
cache contention and SMT sharing inflate it, and a VM's steal time can show
up as guest CPU. Best-of-3 absorbs a single bad sample. Measured on a
10-core Apple Silicon machine whose load average was already 11 to 22:

| test | idle CPU | 20 `yes` hogs, 6 runs | budget |
|---|---|---|---|
| crayon scene | 0.61 s | 0.65-0.70 s | 2 s |
| charcoal hatch | 0.41 s | 0.39-0.43 s | 2 s |
| chalk board | 0.26 s | 0.24-0.27 s | 2 s |
| 100 dkstrokes | 0.26 s | 0.27-0.30 s | 2 s |
| `paintkit` hang guards | 0.005-0.02 s | same | 0.9-5 s |

Proof of the acceptance criterion: the budget tests alone, 6 iterations
under 20 `yes` hogs (0 failures), then the whole `cargo test --profile ci`
suite 3 times under the same 20 hogs while the machine's load average stood
at 17-35 (0 failures; crayon scene 0.68, 0.68, 0.72 s). The hogs were
started with their PIDs recorded and killed by PID afterwards.

The DRYMEDIA budgets keep 3-8x headroom: a real 2x regression (crayon scene
at 1.3 s) still passes, a 3.5x one fails. The `paintkit` hang guards have
200x or more headroom by design: what they catch is a walk-everything
regression (the fan test's comment records ~24 s of nested work), not drift.
The budget values are unchanged from before.

**The negative check.** `PSCAT_TEST_BUDGET_SCALE=0.001 cargo test --profile
ci --test <file> <name>` multiplies every budget the test uses (the helper
and the artkit relative bound). Every converted test fails under it with its
budget message; the `pkfan` full-splay test needs `0.0001` because it
measures about 5 ms. Do not set the variable for a whole-suite run: the
helper's own tests and every other budget test would fail too.

**Not done.** Timing in `benches/` and `scripts/` is out of scope (the issue is about `tests/`).
