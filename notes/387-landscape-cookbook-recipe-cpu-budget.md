# 387: the landscape cookbook's per-recipe CPU budget had no headroom on CI

Same problem as #386 (the mathkit specimen): `every_recipe_renders_clean_and_inside_its_budget`
gave each recipe 2 s of thread CPU time (best of 3), and the GitHub runner is about 3x slower than
the dev machine. Local CPU under `--profile ci`: forest about 0.72 s (after #383's crest light),
evergreens about 0.67 s, clouds 0.52, mist 0.43, the rest under 0.2. On the runner the two slowest
land near 2 s.

## Change

`RECIPE_CPU_BUDGET` in `tests/landscape_cookbook.rs`, 2 s -> 5 s, with a doc comment (2.5x the
estimated runner time, 7x local). The cookbook's "Keeping the recipes honest" paragraph and
HANDOFF.md say 5 s. The 2 s CI cap in DRYMEDIA 6.2 is drykit's, not these recipes'.

## Negative check

`PSCAT_TEST_BUDGET_SCALE=0.0001 cargo test --profile ci --test landscape_cookbook every_recipe`
still fails (recipe `sky`, 28 ms against 0.5 ms scaled), so the budget is still applied.
Baseline-relative budgets that would not need this tuning are tracked in #333.
