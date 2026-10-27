## CPU budgets on a dev-profile build (issue #886, 2026-10-26)

A plain `cargo test` failed `landscape_cookbook` (forest 6.6 s of 5 s),
`the_specimen_renders_with_ink_and_a_clean_stack` (mathkit, 6.6 s of 5 s),
`pigmix_is_cheap` (1.40 s of 1 s) and `a_strip_types_within_the_cpu_budget`
(2.3 s of 2 s) every time, not only under load: the budgets are stated for
`--profile ci` (optimised), where the same renders take 0.03-0.5 s, and a
dev build runs 11-18x slower (the strip test up to 35x loaded). The fifth, a child-pipe deadline in
`block_discovery_agrees_with_the_parser_about_strings`, passes in 0.3 s on a
quiet machine; it only missed the 60 s wall deadline on a loaded one.

**Choice.** Scale, not skip. In an unoptimised build `within_cpu_budget`
multiplies its absolute budget by `UNOPTIMISED_BUDGET_FACTOR` (20, above
the quiet-machine ratios; the strip test still has 9x headroom at its loaded 35x) and `default_deadline()` is 3x (180 s).
The render still runs and the assertion still catches a catastrophic
regression. Instruction counting was not used: no portable counter.

**Detecting the profile.** `cfg!(debug_assertions)` is on in `[profile.ci]`
too, and `build.rs` is an owner path, so the helper reads the directory of
`current_exe()` (`target/<profile>/deps/`): only `debug` is unoptimised;
`ci`, `release`, custom profiles and an unreadable path stay strict.

**`--profile ci` unchanged.** Budgets print the same (5.000 s, 1.000 s,
2.000 s). `PSCAT_TEST_BUDGET_SCALE` multiplies on top and still fails the
tests in both profiles. A bound relative to a same-run baseline
(`artkit`'s walkpath test) keeps `scale_budget` only. `cpu_budget_helper`'s
mechanics tests use `within_exact_cpu_budget`, which skips the factor.

**Not changed.** Two wall-clock assertions in `tests/mcp.rs` (`took < 15 s`,
`started.elapsed() < 12 s`) bound a server `--timeout`, not render speed,
and are not profile-sensitive.
