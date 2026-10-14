# 386: the mathkit specimen's CPU budget had no headroom on CI

Main run 38551816510 (head 743b76d) failed only `the_specimen_renders_with_ink_and_a_clean_stack`:
best CPU 2.013 s (tries 2.032, 2.013, 2.225) against a 2 s budget. The re-run passed.

## Measurements

Thread CPU time under `--profile ci`, best of 3, via
`PSCAT_TEST_BUDGET_SCALE=0.0001 cargo test --profile ci --test mathkit the_specimen -- --nocapture`:

| commit | best |
|---|---|
| 7ab1705 (mathkit lands) | 0.682 s |
| a29030d (#339) | 0.667 s |
| 10daba9 (#366) | 0.671 s |
| 6812313 (#365) | 0.666 s |
| 743b76d / main | 0.665 s |

Two later samples (24355c8 0.985 s, 23022ad 0.902 s) were taken at load average above 10 and
24355c8 is a paintkit-only commit; they are noise. No growth from added glyphs or load cost: the
runner is about 3x slower than the dev machine, and the 2 s cap was only ever 3x the local time.

## Change

Budget 2 s -> 5 s (2.5x the observed runner time, 7.5x local; same size as the paintkit hang
guards). DRYMEDIA 6.2's 2 s is for `tests/drykit*.rs`; an `examples/` specimen gets 10 s there.

## Other budget tests

`strokeglyphs.rs` and `strokepath.rs` have no CPU budgets. Local best-of-3 for the other absolute
budgets: drychalk 0.155 s, drycharcoal 0.227, drycrayon 0.350, drykit 0.169, pktrowelplane 0.262,
pigment 0.159 (budget 1 s), landscape sky 0.028, paintkit hang guards 0.005-0.012 s. The tightest,
drycrayon, has about 5.7x headroom locally and 1.9x at the runner's 3x, so left alone.
Baseline-relative budgets that would not need this tuning are tracked in #333.
