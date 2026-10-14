## work-issue round 7: no pattern kills, identity_check --loader, settling and late rebase (issue #303, 2026-10-14)

Round 7 of the work-issue fixes: the follow-up comments on #276 plus the #110, #271, #279, #281,
#285 and #293 runs. `scripts/claim.sh` and `heartbeat.sh` are untouched (claim redesign is #297).

**What changed**
- `SKILL.md` (236 -> 241 lines): Rule 1 forbids `pkill`/`killall`/`pgrep`-driven kills by pattern
  (record `$!`, kill only that PID); step 4.2 counts tests and notes in the diff size; step 5.5 says
  to rebase as late as possible and what to redo; 8.12/8.13 re-query `merge-tree` and the
  eligibility check after the watch and immediately before the merge; step 9 removes extra
  step-5 worktrees; the Codex line says it cannot run harnesses.
- `references/pitfalls.md`: checklist item 4 rewritten for `--loader`; the redo matrix in "Rebase and
  merge rules"; a "Merge state settles late" CI-wait bullet; the 600 s foreground cap; a new
  "Added in #303" section (pattern kills, gojq vs jq, probe-harness trap, `packedarraytype`
  guards, zsh arrays, timing assertions, max-sweeps, extra worktrees). TOC regenerated.
- `references/finish.md`: step 9 block removes recorded extra worktrees by exact path.
  `references/demo-surfaces.md`: no-merge specimen PRs commit and embed their render pinned to
  the head SHA after the final rebase. `references/codex-review.md`: Codex is read-only.
- `.claude/skills/work-issue/scripts/sweep-max.sh` (+ `test-sweep.sh`): a per-file-hook sweep for
  "the max value fed to X across all pieces".
- `scripts/identity_check.sh`: `--loader LIB` (repeatable) and `--files LIST`.

**identity_check design.** `--loader` is TRANSITIVE: a change to `lib/artkit.ps` reaches most pieces
through paintkit, drykit, ..., so a direct-grep pick would give false confidence (63 files vs the
few that name artkit). The libs found are printed on stderr. The default (no `--loader`, no
`--files`) keeps the old direct paintkit match and every message byte for byte; this was checked by
running the old and new scripts on the same trees and diffing the output. Multi-page `page-NNN.png`
output was already compared page by page (`ls page*.png`); this is now documented and exercised
(`selftest/drivers/artkit.ps` is 8 pages).

**Verified against main itself** (base = detached worktree at the merge-base with a
`/ic303probe true def` marker appended to the lib, uncommitted; the control draws by
`/ic303probe where`): `--loader lib/artkit.ps` 79 of 79 page renders identical across 63 files,
control differs; `--loader lib/hatchkit.ps` 3 of 3 identical, control differs.

**Deferred.** Nothing.
