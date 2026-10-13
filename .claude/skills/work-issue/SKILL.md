---
name: work-issue
description: Run this repo's issue -> feature branch -> PR -> review -> merge -> cleanup SDLC (SDLC.md) end to end for one GitHub issue -- pick or accept an issue, label it in-progress, create a feature branch and worktree, implement it, open a PR, independently review it, and merge per SDLC.md's policy. Use whenever the user says "work on issue N", "implement the next ticket/issue", "pick up the next backlog item", "/work-issue", or similar -- this is the standard way backlog issues in this repo get turned into shipped code, not an ad hoc one-off.
---

# work-issue — one issue, start to finish

Follow the steps in order, exactly. Argument: optional issue number `<N>`.
When a step says **Read X now**, read that whole file (path from the repo
root) before running the step's commands.

**Rules for every step:**
- Replace `<N>`, `<PR#>`, `<PR URL>`, `<R>`, `<SCRATCH>` with real values. Never type the angle brackets.
- Every Bash call is a fresh shell, so variables do not survive. Start each Bash call
  that needs them with `WORKTREE_DIR=<absolute path>; BRANCH=<branch>`.
- After step 3, work only inside `$WORKTREE_DIR`. Never edit the main checkout. Never `git stash`
  there (`refs/stash` is shared by all worktrees): use `git show origin/main:path` or `cp`.
- **Heartbeat** — run this when a step says "heartbeat", and before any long command:
  ```sh
  H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"
  ```
  If it fails with "No such file or directory": stop, never `mkdir` the lock dir.
  Re-run the claim (step 1); continue only if it says `claimed`.
- **Scratch files** go in `<SCRATCH>`: the scratchpad directory your system prompt names, else
  `${TMPDIR:-/tmp}` (write the literal path). Name them `<kind>-<N>.*` (`gate-`, `ci-`,
  `codex-review-`, `issue-<N>-summary.md`); never a relative path escaping the worktree. They die
  with the session: decide a resume from GitHub state.
- macOS has no `timeout`, and foreground `sleep` may be blocked: pitfalls.md "Waiting and timeouts".
- Never end a run with CI pending unless step 8.12's "pending CI" rule applies.
- Read `.claude/skills/work-issue/references/pitfalls.md` once now. (`references/rationale.md`
  explains why each step is shaped as it is; read it before editing this skill.)

## 1. Pick and claim the issue

Record the start time for the final report: `date '+%Y-%m-%d %H:%M:%S %Z'`.

The **open-PR list** (numbers already claimed by an open PR's `Closes #N`) is used below:
```sh
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```
**If `<N>` was given:** `gh issue view <N> --json state,title,labels`, then the open-PR list.
- Closed, or `in-progress` AND in the open-PR list → stop, tell the user.
- `in-review` AND in the open-PR list → a **resume**: claim below, skip step 2 (never re-add
  `in-progress`), follow pitfalls.md "Resume paths" (restores the PR worktree; step 3's
  new-branch route would lose the PR's work), then step 8 where it says.
- Otherwise claim `<N>` (below).

**If no `<N>` was given:** first look for an abandoned run to resume:
```sh
# in-progress issues with no open PR claiming them = crashed-or-still-live prior runs
gh issue list --state open --label in-progress --json number,title
```
For each `in-progress` issue not in the open-PR list, lowest number first, claim it.
Resume the first that says `claimed` (step 3 reuses its worktree). If none, pick a fresh one:
```sh
# candidates: open, not in-progress, lowest number first
gh issue list --state open --search "-label:in-progress" \
  --json number,title,labels --jq 'sort_by(.number)'
```
Claim the first candidate not in the open-PR list. None → report "backlog clear", stop.

**Claim:** Read `.claude/skills/work-issue/references/claim-protocol.md` now. Run its
"Claim the issue" script exactly as written, in one Bash call, replacing only `<N>`. Then:
- `RESULT=claimed*` → you own `<N>`. Continue.
- `RESULT=live*` or `RESULT=lost*` on a resume candidate → claim the next one (then the fresh picker).
  On an explicit `<N>` or a fresh pick → stop, report `<N>` is being worked on, try nothing else.

## 2. Mark it in-progress

```sh
gh label create in-progress --color FBCA04 --description "Actively being worked on" 2>/dev/null || true
gh label create in-review --color 0E8A16 --description "Implementation done, PR open, awaiting human review" 2>/dev/null || true
gh issue edit <N> --add-label in-progress
```

## 3. Branch and worktree

**If a worktree for `<N>` already exists** (`git worktree list` shows a
path ending `-issue-<N>`): set `WORKTREE_DIR` to that absolute path and run:
```sh
BRANCH="$(git -C "$WORKTREE_DIR" branch --show-current)"
LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
```
Do not recompute the branch name from the title. Skip to step 4 (resume: step 8).

**Otherwise** run this from the main checkout, filling in `SLUG` and `PREFIX`:
```sh
git fetch origin main
REPO_DIR=$(basename "$(git rev-parse --show-toplevel)")
SLUG=<kebab-case slug of the issue title, ~40 chars, no leading/trailing hyphens>
PREFIX=feature   # or "fix" if the issue carries the `bug` label — AGENTS.md's convention
BRANCH="${PREFIX}/<N>-${SLUG}"
WORKTREE_DIR="../${REPO_DIR}-issue-<N>"
git worktree add --no-track -b "$BRANCH" "$WORKTREE_DIR" origin/main
(cd "$WORKTREE_DIR" && pwd)   # note this absolute path: it is WORKTREE_DIR from now on
LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
```
Then re-read `$WORKTREE_DIR/.claude/skills/work-issue/SKILL.md` (it may have changed since invocation) and follow that copy.

## 4. Plan, then get the plan reviewed

1. Read the issue: `gh issue view <N> --json title,body,comments`. Make design calls yourself; flag only what is hard to reverse.
2. Estimate size. **Small** = diff of 20 lines or fewer AND no API change (no new/changed
   public Rust item, PostScript operator, CLI flag or lib procedure name). Otherwise **full**.
   This is the only definition of small.
3. Write the plan in your reply: files, approach, what you are not doing.
4. **Hard gate, no code before it.** Heartbeat, then call the `advisor` tool (small: one short
   call, or skip and say so). If it objects, revise the plan. Heartbeat. The report's
   `Plan review` line must say `done` or `skipped (small)`.

## 5. Implement

1. Read every file before you Edit it. Follow AGENTS.md's "Code quality bar"; write notes in `notes/<N>-<slug>.md` (never NOTES.md), update HANDOFF.md/README.md.
2. If a human could look at the result (art, template, chart, font), read
   `.claude/skills/work-issue/references/demo-surfaces.md` now and do what it says.
3. Commit at checkpoints; heartbeat at each. Evidence sweeps, scratch dirs: pitfalls.md "Evidence runs and scratch hygiene".
4. Deferred work becomes an issue, not just a notes line (keep the URLs for the report):
   `gh issue create --title "<title>" --body "<what and why; found while working #<N>>"`.
   If measurement disproves the premise: comment the data on the issue, say so atop the PR, file a follow-up.
5. Quality gate. Before the first push: `git -C "$WORKTREE_DIR" fetch origin main && git -C
   "$WORKTREE_DIR" rebase origin/main`, so breakage already on `main` cannot fail this issue
   (a no-op when up to date; conflicts, or a PR that exists: pitfalls.md "Rebase and merge rules").
   Do pitfalls.md "Step 5 checklist" (cheap checks first). Heartbeat, then run with
   `run_in_background: true` (over 120s; run it in the foreground if the last gate took under ~60 s):
   ```sh
   rm -f <SCRATCH>/gate-<N>.log && cd "${WORKTREE_DIR:?}" && { PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check; } > <SCRATCH>/gate-<N>.log 2>&1; echo "GATE_EXIT=$?" >> <SCRATCH>/gate-<N>.log
   ```
   Wait for the completion notification or a Monitor (`ToolSearch select:Monitor`); never
   `sleep` or `ScheduleWakeup`. `tail -40 <SCRATCH>/gate-<N>.log` must end with `GATE_EXIT=0`; else fix, re-run. Then heartbeat.

## 6. Review the implementation

1. `git -C "$WORKTREE_DIR" diff --numstat origin/main...HEAD` — measure the size.
2. Small (step 4): one short self-check or `advisor` call. Otherwise heartbeat, then `advisor`,
   asking it to check state mutated across branches (gsave/dict unwinding on every error
   path, font/graphics state changed mid-loop), multi-byte/encoding edge cases, hidden side effects
   (non-PostScript diff: say "not applicable").
3. Fix real findings, re-run the gate (5.5), heartbeat. Don't loop on subjective feedback.

## 7. Open the PR

```sh
git push -u origin "$BRANCH"
gh pr create --title "<summary>" --body "$(cat <<'EOF'
## Summary
<what changed and why, in a few bullets>

Closes #<N>

## Test plan
- [x] PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check
<+ whatever else was actually run/verified>
EOF
)"
```
Always push with that explicit refspec, never a bare `git push`. Then:
```sh
gh issue edit <N> --remove-label in-progress --add-label in-review
gh issue comment <N> --body "Opened <PR URL>."
```

## 8. Independent review, then merge

Read `.claude/skills/work-issue/references/codex-review.md` now (full policy). Then:
1. Read the `merge_policy` frontmatter in `SDLC.md` fresh.
2. If the diff touches `.claude/skills/work-issue/`, say so in every review comment.
3. Find Codex: `CODEX_SCRIPT=$(find "$HOME/.claude/plugins" -path "*/codex/scripts/codex-companion.mjs" 2>/dev/null | head -1)`.
   Empty or unauthenticated (`node "$CODEX_SCRIPT" status --json`): pitfalls.md stale-broker checks, then codex-review.md Fallback.
4. Run the review as a **foreground blocking** Bash call (timeout 600000), never
   `run_in_background`, never `ScheduleWakeup` in this turn, never from the main checkout (if the harness backgrounds it anyway: pitfalls.md). Start the
   call with `WORKTREE_DIR=...; BRANCH=...` and item 3's `CODEX_SCRIPT=$(find ...)` line, then:
   ```sh
   rm -f <SCRATCH>/codex-review-<N>.json && \
     H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat" && \
     date +%s > "$H.tmp" && mv "$H.tmp" "$H" && \
     cd "$WORKTREE_DIR" && git fetch origin main && \
     test "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" && \
     node "$CODEX_SCRIPT" review --wait --json --model gpt-6.1-sol \
     --scope branch --base origin/main \
     > <SCRATCH>/codex-review-<N>.json
   ```
5. Check it succeeded; if not, do not post from the file (as in 3):
   `jq -e '.codex.status == 0 and (.codex.stdout // "" | length > 0)' <SCRATCH>/codex-review-<N>.json`
6. Post it on the PR, even when it is clean:
   `gh pr comment <PR#> --body "$(jq -r '"## Codex review\n\n" + .codex.stdout' <SCRATCH>/codex-review-<N>.json)"`
7. Read every finding. Decide for each: fix it, or state why not.
8. Post the round on the issue (`## Codex review, round <R>` template in codex-review.md).
9. If you fixed anything: commit, `git push` (no force), re-run the gate (5.5), go back to 4
   with R+1. Exception for a doc-only merge/rebase: pitfalls.md "Rebase and merge rules".
10. Merge eligibility, from `merge_policy.mode`:
    - `human-only`, or `size-and-risk-bar` and over the bar: `rm -rf "$(git rev-parse
      --path-format=absolute --git-common-dir)/work-issue-lock-<N>"`, go to step 10 (awaiting merge). Stop.
    - `agent-full`, or under the bar: continue, unless the dispatcher said not to merge: do 11, release
      the lock as above, keep `in-review` and the worktree, stop (pitfalls.md "Stop without merging").
11. Write `<SCRATCH>/issue-<N>-summary.md`, starting `## Summary of work (PR #<PR#>)` with the sections
    in codex-review.md, then `gh issue comment <N> --body-file <SCRATCH>/issue-<N>-summary.md`
12. First, in `$WORKTREE_DIR`: `git fetch origin main && git merge-tree --write-tree origin/main HEAD >/dev/null`;
    non-zero = conflicts: merge `main` locally now (pitfalls.md "Rebase and merge rules"), push, then wait.
    `gh pr view <PR#> --json mergeable,mergeStateStatus,statusCheckRollup`. `BEHIND` does not block
    merging (`require_up_to_date: false`): never update the branch for it. `UNKNOWN` is not yet computed,
    not a failure: re-query up to 5 times, ~10 s apart. `DIRTY`: `gh pr update-branch` refuses, merge
    locally. If a check is pending, heartbeat, then run this with `run_in_background: true` (~2-3 min):
    ```sh
    gh pr checks <PR#> --watch --fail-fast > <SCRATCH>/ci-<N>.log 2>&1; echo "exit=$?" >> <SCRATCH>/ci-<N>.log
    ```
    Wait for the notification; no `sleep` loops, no `| tail`, no hand-back while it runs. `exit=` is a
    hint only. Heartbeat after every watch, then merge only if this prints `true` (`UNKNOWN` prints `false`):
    `gh pr view <PR#> --json mergeable,mergeStateStatus,statusCheckRollup --jq '(.mergeable == "MERGEABLE" and .mergeStateStatus != "DIRTY") and ([.statusCheckRollup[] | (.conclusion // .state)] | (length > 0 and all(. == "SUCCESS")))'`
    Otherwise see pitfalls.md "CI wait" (pending, "no checks reported", infra failure: `gh run
    rerun --failed`; a failed step: stop and report). Never merge past a failure or use `--admin`.
    Cannot wait: pitfalls.md "Pending-CI hand-back".
13. `gh pr merge <PR#> --squash` (no `--delete-branch`: the repo deletes the remote branch; step 9 the local).

## 9. Post-merge cleanup

One Bash call; the first line aborts it unless the PR is `MERGED`. Never reorder it (worktree before branch).
```sh
test "$(gh pr view <PR#> --json state --jq .state)" = MERGED || { echo "PR not MERGED: stop, no cleanup"; exit 1; }
gh issue view <N> --json state   # confirm Closes #<N> auto-closed it
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
Then confirm main CI for your merge commit (pitfalls.md "Main CI after merge"). Red: stop, report it first, start no other work.

## 10. Final report

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
