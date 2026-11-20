# #1336: the "shfill legs" CPU budget flaked on the Windows runner

`a_ramp_of_thousands_of_legs_paints_in_bounded_time` (tests/limits.rs) held a
4095-leg `shfill` at 300 dpi to 2 s of CPU. On Windows the test helper has no
per-thread CPU clock and falls back to wall time since process start
(`common::thread_cpu_time`), and the runner is several times slower than the
unix ones (0.25-0.8 s there). A failing run printed tries of 2.000 s, 2.357 s
and 2.356 s.

Now:
- The test asserts the work the `shfill` charges against the step limit
  (`shfill_work`, #1219: a function of the stops the raster is given), as a
  ratio: 4095 legs against 511 legs (1022 stops, never resampled). Resampled
  to 1024 stops the ratio is 1.00 (128,779 vs 128,306 steps on a 1 pt/pixel
  page); with `MAX_RASTER_STOPS` lifted it is 7.61. The bound is 2, through
  `scale_budget`. No clock, so it holds on every platform and profile.
- The 2 s CPU budget stays as `a_ramp_of_thousands_of_legs_takes_bounded_cpu`,
  `#[cfg(unix)]` only, where `thread_cpu_time` is real CPU time. It covers what
  the step charge cannot see: a raster built from more stops than the charge
  counts.

Other `within_cpu_budget` calls in tests/limits.rs (the one-operator and
recorder tests) run on the same wall-clock fallback on Windows. They have not
flaked; if one does, give it the same treatment or teach `common` that the
clock is not CPU time there.
