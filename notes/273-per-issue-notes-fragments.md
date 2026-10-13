## Per-issue notes fragments, main CI no longer cancels itself (issue #273, 2026-10-13)

Two causes of wasted cycles under parallel agents, both seen repeatedly on
2026-10-13 (#233, #258, #263, paintkit L3).

- **Notes.** New entries go in `notes/<issue>-<slug>.md`, one file per
  issue (see `notes/README.md`), so concurrent PRs never conflict on
  `NOTES.md`. Existing `NOTES.md` content stays where it is. No index file is
  regenerated: an index would itself be a shared file and bring the
  conflict back. `AGENTS.md`, `HANDOFF.md` and `/work-issue` (step 5.1, the
  rebase rule in pitfalls) were updated to match.
- **CI.** `ci.yml` had `cancel-in-progress: true` on a per-ref group, so on
  a busy `main` each merge commit's push run was cancelled by the next merge.
  Now PRs keep cancel-in-progress (their runs post a PR comment, and an
  older run finishing last would overwrite the newer evidence), while a push
  run is keyed by its commit SHA with cancel off. The SHA key matters:
  with a shared `main` group and cancel off, GitHub still cancels a
  *pending* run when a newer one queues, so three quick merges would still
  lose the middle one. Push runs post no comments, so there is nothing to
  race.
- **`/work-issue`.** Step 8.12's merge query now requires
  `mergeable == "MERGEABLE"`, because `mergeStateStatus` reads `UNKNOWN`
  for a few seconds after a push and `UNKNOWN != "DIRTY"` is true (the
  likeliest reason the #263 agent saw `true` on a conflicted PR; the copy of
  SKILL.md in the repo already had the `!= "DIRTY"` guard). A
  `git merge-tree` conflict check runs before waiting on CI. Main-CI check
  is a bounded wait loop on the merge commit's own run. Smaller items are in
  pitfalls.md ("Evidence runs and scratch hygiene").
