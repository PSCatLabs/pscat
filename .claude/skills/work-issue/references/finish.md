# Steps 9 and 10: post-merge cleanup and the final report

Read by `SKILL.md` steps 9 and 10 (moved here from `SKILL.md` in #276 to keep it at or
under 250 lines). Step numbers refer to `../SKILL.md`; "Pitfalls" means `pitfalls.md`.

## Step 9. Post-merge cleanup

Step 9 runs from the **main checkout**, never from `$WORKTREE_DIR` (the block removes
that worktree; a shell standing in it breaks). Shell state does not persist, so start the
Bash call by re-declaring `WORKTREE_DIR` (absolute path) and `BRANCH`, and `cd` to the
main checkout (`git worktree list` shows it first).

One Bash call; the first line aborts it unless the PR is `MERGED`. Never reorder it
(worktree before branch). If `Closes #<N>` did not close the issue (seen on #224 even
with a correct `Closes` line), the second line closes it with a comment linking the PR.
```sh
WORKTREE_DIR=<absolute path>; BRANCH=<branch>
cd "$(git worktree list --porcelain | sed -n '1s/^worktree //p')" || exit 1   # the main checkout
test "$(gh pr view <PR#> --json state --jq .state)" = MERGED || { echo "PR not MERGED: stop, no cleanup"; exit 1; }
[ "$(gh issue view <N> --json state --jq .state)" = OPEN ] && gh issue close <N> --comment "Merged in <PR URL>."
gh issue view <N> --json state   # must print CLOSED
gh issue edit <N> --remove-label in-review --remove-label in-progress
gh issue view <N> --json labels --jq '[.labels[].name]'   # must list neither in-review nor in-progress
gh issue view <N> --json comments --jq '[.comments[].body | select(startswith("## Summary of work"))] | length'   # both counts >= 1
gh issue view <N> --json comments --jq '[.comments[].body | select(test("^## (Codex|Fallback) review, round"))] | length'
git worktree remove "$WORKTREE_DIR" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" branch -D "$BRANCH" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" fetch origin --prune
git -C "$(git rev-parse --show-toplevel)" ls-remote --exit-code --heads origin "$BRANCH" >/dev/null 2>&1 && git -C "$(git rev-parse --show-toplevel)" push origin --delete "$BRANCH"   # only if still there
rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
```
Then confirm main CI for your merge commit (pitfalls.md "Main CI after merge"). Red:
stop, report it first, start no other work.

## Step 10. Final report

End the run by printing exactly this, filled in (write `none` where empty):
```text
## work-issue report: #<N> <title>
- PR: <PR URL>
- Merge: merged as <merge commit sha> | awaiting human merge (<why>) | pending CI (<remaining steps>) | blocked (<why>)
- Plan review (step 4): <done | skipped (small)>
- Checks: quality gate <pass/fail>; PR CI <pass/fail>; main CI after merge <pass/fail/n.a.>
- Wall time: <start time from step 1> -> <now> (<minutes> min)
- Review rounds: <count>, reviewer <Codex | fallback (why)>, findings fixed <n> / declined <n>
- Steps skipped or failed: <step: reason>
- Issues filed: <URLs from gh issue create>
- Cleanup: worktree <removed/kept at path>, branch <deleted/kept>, lock <released>, labels <cleared>
- Friction: <anything in this skill that was unclear, wrong or slow>
```
