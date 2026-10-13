## work-issue round 6: claim and heartbeat as scripts (issue #276, 2026-10-13)

Two `/work-issue` runs in a row (#273, #224) ran only the fresh-claim `mkdir` branch of the pasted
step 1 claim block. Both got `claimed (fresh)`, but the block is race-tested as a whole, so the
fix is structural: the commands now live in files an agent runs as one command.

- **`scripts/claim.sh <N>`** is the claim block from `claim-protocol.md`, moved verbatim with the
  original comments. The only differences are `<N>` becoming `$N` and a leading argument check
  (an empty `N` would have claimed `work-issue-lock-` for everyone). No `set -e`/`-u`: the
  missing-heartbeat path relies on `$(cat ...)` being allowed to fail. `scripts/heartbeat.sh <N>`
  is the two-line refresh, still never `mkdir`ing the lock.
- **`scripts/test-claim.sh`** re-runs the race tests in a throwaway git repo, with a start barrier
  so racers really overlap: 30 fresh and 20 stale-reclaim races, the two `DIR_AGE` cases, garbage
  heartbeat, live heartbeat, a delayed-racer ABA case (a `sleep 1` injected into a copy), and
  `heartbeat.sh`. `CLAIM_ORIG` diffs the script body against the pre-#276 block.
- **Finding: a stale reclaim race can have zero winners.** One trial in ~100 ended with every
  racer reporting `lost`: a racer's ABA restore (`mv "$STEAL" "$LOCKDIR"`) landed on another
  racer's in-flight reclaim, whose heartbeat `mv` then failed. No double claim and the lock stays
  reclaimable after the 10 s grace window; the harness asserts "never two winners", not "exactly
  one", for stale races. This is behaviour of the original protocol, not of the move.
- Other fixes: `Closes #<N>` is mandatory in the PR body, step 9 closes a still-OPEN issue with a
  comment, the worktree SKILL.md is re-read after the 5.5 rebase, step 9 runs from the main
  checkout and re-declares `WORKTREE_DIR`/`BRANCH`, a gate stands after a rebase that only
  brought in docs/skill/CI files (an allowlist, so `scripts/` re-triggers it), and the
  `identity_check.sh`, `--headless`, `build.rs` `/name N dict def` and library-specimen notes.
- **SKILL.md budget:** step 9's block and step 10's template moved to `references/finish.md`;
  SKILL.md is 232 lines, was 250.
