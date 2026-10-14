## work-issue: close without a PR, prototypes, timing guidance (issue #331, 2026-10-14)

Skill-only change under `.claude/skills/work-issue/`; no `src/`, `lib/` or test changes.

- **Close without a PR** (`references/finish.md`): a runnable block for a run that ends with nothing
  worth committing: `wontfix` label, close as not planned, clear `in-progress`/`in-review`, remove the
  worktree and scratch worktrees by exact path, delete the local (and, only if pushed, the remote)
  branch, release the lock. Step 10 gained a `closed without a PR` Merge value. A finding worth
  shipping still uses the `Refs #N` won't-change PR.
- **Prototypes and timing** (`references/pitfalls.md`, new last section): a throwaway measurement
  prototype in a scratch worktree is allowed after the step 4 plan review. Timing is advisory (a
  guard, not a goal), points at `scripts/bench_compare.sh` and the AGENTS.md CPU-time convention, and
  the old "Timing assertions in tests" bullet moved here and was rewritten (it said wall-clock and
  N >= 5; it now says CPU time, `within_cpu_budget` for a documented cap, a self-relative bound
  otherwise, `PSCAT_TEST_BUDGET_SCALE` as the negative check).
- **`scripts/load-run.sh`** (+ `test-load-run.sh`): N CPU hogs around a command, PIDs recorded, only
  those killed on exit/INT/TERM. Only the direct command is killed, so a wrapper that forks must use
  `exec`.
- **Step 5.5**: a merge of `main` after the gate that brings in `src/`/`lib/`/tests means re-run the
  gate or state that CI covers it (rule in pitfalls "Rebase and merge rules").
- **Step 8.12**: a `gh pr checks` probe retried on the text `no checks reported` (not the exit code)
  before the watch.
- **Stop before merge** now waits for CI green (8.12) and stops before 8.13; it used to stop before
  8.12, handing the maintainer a PR with CI possibly running or red.
- **demo-surfaces.md**: record a gallery piece's `gs` invocation (`-dNOSAFER`, or
  `--permit-file-read=<dir>`, with relative paths; checked: default-safe gs raises `invalidfileaccess`
  on `(lib/artkit.ps) run`); re-pin image URLs with `gh pr edit` after merging `main`.
- **pitfalls.md contents**: the line-range table is now a section-name list (`grep -n '^#'` finds
  the offsets), so it cannot drift. SKILL.md stays at 249 lines (cap 250).
