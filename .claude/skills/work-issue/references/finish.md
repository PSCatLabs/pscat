# Steps 9 and 10: post-merge cleanup, closing without a PR, and the final report

Read by `SKILL.md` steps 5.4, 9 and 10 (moved here from `SKILL.md` in #276 to keep it at or
under 250 lines). Step numbers refer to `../SKILL.md`; "Pitfalls" means `pitfalls.md`.

## Step 9. Post-merge cleanup

Step 9 (#303: now also removes the extra step-5 worktrees) runs from the **main checkout**, never from `$WORKTREE_DIR` (the block removes
that worktree; a shell standing in it breaks). Shell state does not persist, so start the
Bash call by re-declaring `WORKTREE_DIR` (absolute path) and `BRANCH`, and `cd` to the
main checkout (`git worktree list` shows it first).

One Bash call; the first line aborts it unless the PR is `MERGED`. Never reorder it
(worktree before branch). If `Closes #<N>` did not close the issue (seen on #224 even
with a correct `Closes` line), the second line closes it with a comment linking the PR.
```sh
WORKTREE_DIR=<absolute path>; BRANCH=<branch>
EXTRA_WORKTREES=()   # exact paths of extra step-5 worktrees (identity base, comparison trees, the base worktree behind --base-bin), e.g. ("/abs/idbase" "/abs/other"); empty if none
cd "$(git worktree list --porcelain | sed -n '1s/^worktree //p')" || exit 1   # the main checkout
test "$(gh pr view <PR#> --json state --jq .state)" = MERGED || { echo "PR not MERGED: stop, no cleanup"; exit 1; }
[ "$(gh issue view <N> --json state --jq .state)" = OPEN ] && gh issue close <N> --comment "Merged in <PR URL>."
gh issue view <N> --json state   # must print CLOSED
gh issue edit <N> --remove-label in-review --remove-label in-progress
gh issue view <N> --json labels --jq '[.labels[].name]'   # must list neither in-review nor in-progress
gh issue view <N> --json comments --jq '[.comments[].body | select(startswith("## Summary of work"))] | length'   # both counts >= 1
gh issue view <N> --json comments --jq '[.comments[].body | select(test("^## (Codex|Fallback) review, round"))] | length'
git worktree remove "$WORKTREE_DIR" 2>&1 || true
# Extra step-5 worktrees: only the exact paths in EXTRA_WORKTREES above, never a pattern over
# `git worktree list` (other agents' `-issue-<M>` trees sit in it). --force: a base tree may carry a
# scratch edit. A no-op when the array is empty.
for W in "${EXTRA_WORKTREES[@]}"; do git worktree remove --force "$W" 2>&1 || true; done
git -C "$(git rev-parse --show-toplevel)" branch -D "$BRANCH" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" fetch origin --prune
git -C "$(git rev-parse --show-toplevel)" ls-remote --exit-code --heads origin "$BRANCH" >/dev/null 2>&1 && git -C "$(git rev-parse --show-toplevel)" push origin --delete "$BRANCH"   # only if still there
rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
```
**Won't-change outcome (#303, from #287/PR #304).** The PR body said `Refs #<N>`, so the issue stays OPEN
after the merge. Replace the `gh issue close` line above with:
```sh
gh label create wontfix --color FFFFFF --description "This will not be worked on" 2>/dev/null || true
gh issue edit <N> --add-label wontfix
gh issue close <N> --reason "not planned" --comment "Not changed, per the measurement in <PR URL>."
gh issue view <N> --json state,stateReason   # must print CLOSED / NOT_PLANNED
```
and keep the rest. A `Refs` PR is not found by step 1's open-PR scan (it only matches Closes/Fixes/Resolves),
so merge it promptly and leave a comment on the issue linking the PR.

Then confirm main CI for your merge commit (pitfalls.md "Main CI after merge"). Red:
stop, report it first, start no other work.

## Close without a PR (#331, from the #316 run)

For a run whose measurement or investigation shows the issue's change should not be made **and
there is nothing worth committing** (the finding fits in an issue comment). If a notes file or
code comment is worth keeping, use the won't-change PR instead (SKILL.md step 7, `Refs #<N>`; the
variant above). Use it from step 5 on, after the data is on the issue; steps 6-9 do not run.
It also fits an issue that turns out to be a duplicate or already fixed.

Like step 9 it runs from the **main checkout**, in one Bash call that re-declares `WORKTREE_DIR`
and `BRANCH` first. Post the findings comment before the block (data, method, the verdict, and any
follow-up issue); closing first loses the reader. Nothing was pushed in the usual case; the remote
branch is deleted only if it exists.
```sh
WORKTREE_DIR=<absolute path>; BRANCH=<branch>
EXTRA_WORKTREES=()   # exact paths of scratch/prototype worktrees you made; empty if none
cd "$(git worktree list --porcelain | sed -n '1s/^worktree //p')" || exit 1   # the main checkout
test -z "$(git -C "$WORKTREE_DIR" status --porcelain)" || { echo "worktree has uncommitted changes: copy anything you need, then rerun"; exit 1; }
gh label create wontfix --color FFFFFF --description "This will not be worked on" 2>/dev/null || true
gh issue edit <N> --add-label wontfix --remove-label in-progress --remove-label in-review
gh issue close <N> --reason "not planned" --comment "Not changed, per the findings above."
gh issue view <N> --json state,stateReason,labels --jq '[.state, .stateReason, ([.labels[].name] | join(","))]'   # CLOSED / NOT_PLANNED, neither in-* label
git worktree remove --force "$WORKTREE_DIR" 2>&1 || true
for W in "${EXTRA_WORKTREES[@]}"; do git worktree remove --force "$W" 2>&1 || true; done
git branch -D "$BRANCH" 2>&1 || true
git ls-remote --exit-code --heads origin "$BRANCH" >/dev/null 2>&1 && git push origin --delete "$BRANCH"   # only if it was pushed
rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
```
`--force` on the worktree discards its edits, which is the point of the check above it. Then print
step 10's report with `PR: none`, `Merge: closed without a PR (<why>)`, `Plan review` as it was, and
`Checks: n.a.` where nothing ran.

## Step 10. Final report

End the run by printing exactly this, filled in (write `none` where empty):
```text
## work-issue report: #<N> <title>
- PR: <PR URL> | none (closed without a PR)
- Merge: merged as <merge commit sha> | awaiting human merge (<why>) | pending CI (<remaining steps>) | blocked (<why>) | closed without a PR (<why>)
- Plan review (step 4): <done | skipped (small)>
- Checks: quality gate <pass/fail>; PR CI <pass/fail>; main CI after merge <pass/fail/n.a.>
- Wall time: <start time from step 1> -> <now> (<minutes> min)
- Review rounds: <count>, reviewer <Codex | fallback (why)>, findings fixed <n> / declined <n>
- Steps skipped or failed: <step: reason>
- Issues filed: <URLs from gh issue create>
- Cleanup: worktree <removed/kept at path>, branch <deleted/kept>, lock <released>, labels <cleared>
- Friction: <anything in this skill that was unclear, wrong or slow>
```
