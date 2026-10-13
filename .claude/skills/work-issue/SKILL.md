---
name: work-issue
description: Run this repo's issue -> feature branch -> PR -> review -> merge -> cleanup SDLC (SDLC.md) end to end for one GitHub issue -- pick or accept an issue, label it in-progress, create a feature branch and worktree, implement it, open a PR, independently review it, and merge per SDLC.md's policy. Use whenever the user says "work on issue N", "implement the next ticket/issue", "pick up the next backlog item", "/work-issue", or similar -- this is the standard way backlog issues in this repo get turned into shipped code, not an ad hoc one-off.
---

# work-issue — one issue, start to finish

Follow the steps in order, exactly. Argument: optional issue number `<N>`.
When a step says **Read X now**, read that whole file (path from the repo
root) before running the step's commands.

**Rules for every step:**
- Replace `<N>`, `<PR#>`, `<PR URL>`, `<R>` with real values. Never type the angle brackets.
- Every Bash call is a fresh shell. Variables do not survive. Start each
  Bash call that needs them with `WORKTREE_DIR=<absolute path>; BRANCH=<branch>`.
- After step 3, work only inside `$WORKTREE_DIR`. Never edit the main checkout.
- **Heartbeat** — run this when a step says "heartbeat", and before any long command:
  ```sh
  H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"
  ```
  If it fails with "No such file or directory": stop. Do not `mkdir`
  the lock dir. Re-run the claim (step 1) and only continue if it says `claimed`.
- Read `.claude/skills/work-issue/references/pitfalls.md` once now. (`references/rationale.md`
  explains why each step is shaped as it is; read it before editing this skill.)

## 1. Pick and claim the issue

Record the start time for the final report: `date '+%Y-%m-%d %H:%M:%S %Z'`.

**If `<N>` was given:**
```sh
gh issue view <N> --json state,title,labels
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```
- Closed, or labeled `in-progress` AND in the open-PR list → stop, tell the user.
- Otherwise → claim `<N>` (below).

**If no `<N>` was given:** first look for an abandoned run to resume:
```sh
# in-progress issues with no open PR claiming them = crashed-or-still-live prior runs
gh issue list --state open --label in-progress --json number,title
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```
For each `in-progress` issue not in the open-PR list, lowest number first,
claim it. Resume the first one that says `claimed` (step 3 reuses its
worktree). If none is claimed, pick a fresh one:
```sh
# candidates: open, not in-progress, lowest number first
gh issue list --state open --search "-label:in-progress" \
  --json number,title,labels --jq 'sort_by(.number)'

# cross-check: numbers already claimed by an open PR's "Closes #N"/"Fixes #N"
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```
Take the first candidate not in the open-PR list and claim it. No
candidates → report "backlog clear" and stop.

**Claim:** Read `.claude/skills/work-issue/references/claim-protocol.md` now.
Run its "Claim the issue" script exactly as written, in one Bash call,
replacing only `<N>`. Then:
- `RESULT=claimed*` → you own `<N>`. Continue.
- `RESULT=live*` or `RESULT=lost*` → stop. Report that `<N>` is being worked
  on. Do not try the next candidate.

## 2. Mark it in-progress

```sh
gh label create in-progress --color FBCA04 \
  --description "Actively being worked on" 2>/dev/null || true
gh label create in-review --color 0E8A16 \
  --description "Implementation done, PR open, awaiting human review" \
  2>/dev/null || true

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
Do not recompute the branch name from the title. Skip to step 4.

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

## 4. Plan, then get the plan reviewed

1. Read the issue: `gh issue view <N>`. Make design calls yourself; flag
   only what is hard to reverse.
2. Estimate size. **Small** = diff of 20 lines or fewer AND no API change
   (no new/changed public Rust item, PostScript operator, CLI flag or
   lib procedure name). Otherwise **full**.
3. Write the plan in your reply: files, approach, what you are not doing.
4. Heartbeat, then call the `advisor` tool (small: one short call is enough).
   If it objects, revise the plan before writing code. Heartbeat.

## 5. Implement

1. Read every file before you Edit it (the Edit tool refuses otherwise).
2. Follow AGENTS.md's "Code quality bar". Update NOTES.md/HANDOFF.md/README.md.
3. If a human could look at the result (art, template, chart, font), read
   `.claude/skills/work-issue/references/demo-surfaces.md` now and do what it says.
4. Commit at checkpoints; heartbeat at each.
5. Deferred work becomes an issue, not just a NOTES.md line:
   `gh issue create --title "<title>" --body "<what and why; found while working #<N>>"`.
   Keep the URLs for the report.
6. Quality gate. It takes longer than 120s, so heartbeat, then run this
   with `run_in_background: true`:
   ```sh
   cd "$WORKTREE_DIR" && { cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check; } > /tmp/gate-<N>.log 2>&1; echo "GATE_EXIT=$?" >> /tmp/gate-<N>.log
   ```
   Wait for the background task's completion notification (or Monitor:
   load it first with ToolSearch `select:Monitor`). Never use `sleep` or
   `ScheduleWakeup`. Then `tail -40 /tmp/gate-<N>.log`. It must end with
   `GATE_EXIT=0`. If not, fix and re-run.
7. Heartbeat.

## 6. Review the implementation

1. `git -C "$WORKTREE_DIR" diff --numstat origin/main...HEAD` — confirm the size.
2. Small (as defined in step 4, now measured): one short self-check or
   `advisor` call. Otherwise: heartbeat, then call `advisor` asking it to check
   state mutated across control-flow branches (gsave/dict unwinding on
   every error path, font/graphics state changed mid-loop),
   multi-byte/encoding edge cases, and hidden side effects.
3. Fix real findings, re-run the step 5 gate, heartbeat. Don't loop on subjective feedback.

## 7. Open the PR

```sh
git push -u origin "$BRANCH"
gh pr create --title "<summary>" --body "$(cat <<'EOF'
## Summary
<what changed and why, in a few bullets>

Closes #<N>

## Test plan
- [x] cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check
<+ whatever else was actually run/verified>
EOF
)"
```
Always push with that explicit refspec. Never run a bare `git push`. Then:
```sh
gh issue edit <N> --remove-label in-progress --add-label in-review
gh issue comment <N> --body "Opened <PR URL>."
```

## 8. Independent review, then merge

Read `.claude/skills/work-issue/references/codex-review.md` now (full policy). Then:

1. Read the `merge_policy` frontmatter in `SDLC.md` fresh.
2. If the diff touches `.claude/skills/work-issue/`, say so in every review comment.
3. Find Codex: `CODEX_SCRIPT=$(find "$HOME/.claude/plugins" -path "*/codex/scripts/codex-companion.mjs" 2>/dev/null | head -1)`.
   If it is empty or `node "$CODEX_SCRIPT" status --json` shows it is not
   authenticated, use the Fallback path in codex-review.md.
4. Run the review as a **foreground blocking** Bash call (timeout 600000).
   Never set `run_in_background`. Never call `ScheduleWakeup` in this turn.
   Never run it from the main checkout. Set the variables first:
   ```sh
   rm -f /tmp/codex-review-<N>.json && \
     H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat" && \
     date +%s > "$H.tmp" && mv "$H.tmp" "$H" && \
     cd "$WORKTREE_DIR" && git fetch origin main && \
     test "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" && \
     node "$CODEX_SCRIPT" review --wait --json --model gpt-6.1-sol \
     --scope branch --base origin/main \
     > /tmp/codex-review-<N>.json
   ```
   If the harness backgrounds it anyway, read the polling pitfalls in pitfalls.md.
5. Check that it succeeded. If this fails, go to the Fallback path. Do not post from the file:
   `jq -e '.codex.status == 0 and (.codex.stdout // "" | length > 0)' /tmp/codex-review-<N>.json`
6. Post it on the PR, even when it is clean:
   `gh pr comment <PR#> --body "$(jq -r '"## Codex review\n\n" + .codex.stdout' /tmp/codex-review-<N>.json)"`
7. Read every finding. Decide for each: fix it, or state why not.
8. Post the round on the issue using the `## Codex review, round <R>`
   template in codex-review.md: verbatim output plus one table row per finding.
9. If you fixed anything: commit, `git push`, re-run the step 5 gate,
   and go back to 4 with R+1.
10. Merge eligibility, from `merge_policy.mode`:
    - `human-only`, or `size-and-risk-bar` and over the bar: run
      `rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"`,
      then go to step 10 and report the PR as awaiting merge. Stop.
    - `agent-full`, or under the bar: continue.
11. Write `/tmp/issue-<N>-summary.md`. It starts with `## Summary of work (PR #<PR#>)`
    and has the sections Delivered / Decisions / Review record / Known
    deviations / open points (see codex-review.md). Then:
    `gh issue comment <N> --body-file /tmp/issue-<N>-summary.md`
12. `gh pr view <PR#> --json mergeable,mergeStateStatus,statusCheckRollup`.
    If `BEHIND`: `gh pr update-branch <PR#>`, then `gh pr checks <PR#> --watch`
    (`run_in_background: true`, then wait for the notification). If CI fails,
    stop and report. Never merge past it.
13. `gh pr merge <PR#> --squash --delete-branch`

## 9. Post-merge cleanup

Run in order. If the first command does not print `MERGED`, stop.
```sh
gh pr view <PR#> --json state --jq .state   # must print MERGED
gh issue view <N> --json state   # confirm Closes #<N> auto-closed it
gh issue edit <N> --remove-label in-review --remove-label in-progress 2>&1 || true
# both must print >= 1: the pre-merge summary and the per-round review trail are on the ticket
gh issue view <N> --json comments --jq '[.comments[].body | select(startswith("## Summary of work"))] | length'
gh issue view <N> --json comments --jq '[.comments[].body | select(test("^## (Codex|Fallback) review, round"))] | length'
git worktree remove "$WORKTREE_DIR" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" branch -D "$BRANCH" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" fetch origin --prune
rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
```
Remove the worktree before deleting the branch. Never change that order.

## 10. Final report

End the run by printing exactly this, filled in (write `none` where empty):
```text
## work-issue report: #<N> <title>
- PR: <PR URL>
- Merge: merged as <merge commit sha> | awaiting human merge (<why>) | blocked (<why>)
- Checks: quality gate <pass/fail>; CI <pass/fail>
- Wall time: <start time from step 1> -> <now> (<minutes> min)
- Review rounds: <count>, reviewer <Codex | fallback (why)>, findings fixed <n> / declined <n>
- Steps skipped or failed: <step: reason>
- Issues filed: <URLs from gh issue create>
- Cleanup: worktree <removed/kept at path>, branch <deleted/kept>, lock <released>, labels <cleared>
- Friction: <anything in this skill that was unclear, wrong or slow>
```
