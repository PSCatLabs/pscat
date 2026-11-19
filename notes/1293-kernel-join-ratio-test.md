## `joins_over_many_names_compile_in_linear_time` as a growth ratio (issue #1293, 2026-11-19)

The test failed once on the `windows-latest` CI runner and passed on rerun. It
asserted an absolute budget (400 ms best of 3). The compile takes about 20-90
ms on a Mac, but the Windows runner is several times slower, and
`common::thread_cpu_time` has no per-thread CPU clock there: it falls back to
wall time, so a noisy neighbour counts in full.

**Fix: compare sizes, not a figure.** The test now compiles the same program
with the names in scope at 1,000 and at 4,000, holding the join count at 1,000,
takes the best of 5 CPU times for each, and asserts the ratio is under 10.
A slower or noisier machine scales both sides, so the ratio holds; best-of-5
absorbs one slow sample. A loose absolute backstop (1.5 s for 1,000 joins over
4,000 names, 15x the Mac figure) stays for a catastrophic regression.

**Why hold the joins fixed.** The first draft grew names and joins together.
That is quadratic even in the fixed compiler (measured 14.5x for 4x), which is
too close to a regression to separate. Per join cost is what #1094 made linear in the names, so only the names
vary.

**Measured** (`--profile ci`, M-series Mac):

| compiler | 1,000 names | 4,000 names | ratio |
|---|---|---|---|
| current | 18-20 ms | 66-90 ms | 3.1-6.2 over 20 runs under load, both modes |
| before #1094 (`e246b80~1`) | 1.12 s | 16.7 s | 14.9 (the test fails) |

The bound of 10 sits between 6.2 and 14.9. `PSCAT_TEST_BUDGET_SCALE=0.001`
makes the test fail on the ratio assertion, as the timing-test guidance asks.

Not done: a real per-thread CPU clock for Windows (`GetThreadTimes`) in
`tests/common/mod.rs`. It would help every CPU-budget test on that runner;
it could not be checked here, only on CI.
