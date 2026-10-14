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

**Verified against main itself** (after the final rebase, which pulled in `lib/drykit.ps`, so the base was
re-created from the new merge-base and the checks re-run). Base = detached worktree at the merge-base
with a `/ic303probe true def` marker appended to the lib (uncommitted); the control draws by
`/ic303probe where`. Same binary for both trees.
- `--loader lib/artkit.ps`: 79 of 79 page renders identical across 63 files (includes the 8-page
  `selftest/drivers/artkit.ps`), control differs, exit 0.
- `--loader lib/hatchkit.ps`: 3 of 3 identical, control differs, exit 0; without the marker the control
  does not differ and the exit is 2.
- Default (no flag): output identical to the pre-change script on the same trees (diff of both logs, timings
  normalised): 33 of 33 pages across 24 files, exit 0.
- `--files` with a base `setrgbcolor` redefinition: 7 of 8 pages of the multi-page driver DIFFER, exit 1;
  a bad path in the list exits 64 before any scratch directory is made.
- Also: `test-scan.sh`, `test-sweep.sh` and a short `test-claim.sh` run (ALL PASS); `claim.sh`/`heartbeat.sh`
  unchanged; full gate green after the rebase.

**Won't-change outcome** (coordinator addition, from #287/PR #304): `Refs #N` in the PR body, small for the
plan gate, step 9 closes with `--reason "not planned"` plus `wontfix` (finish.md). `gh pr checks --watch`
logs are read with `tail -n 25`.

**Review fixes (advisor).** `sweep-max.sh` passed the file to the hook twice (test hook now fails unless it
gets exactly one argument); the finish.md `EXTRA_WORKTREES` is an array, not an `<...>` placeholder that is a
shell parse error; `--files` paths are validated before the scratch dir exists; `--gallery-still` dies early
when the pick leaves out `alpine_lake`.

**Deferred.** Nothing.
