# Why the steps are the way they are (steps 1-7 and 9)

Background reading, not a procedure. This is the original prose of the
pre-#194 `SKILL.md` for the steps whose detail did not move to
`claim-protocol.md`, `codex-review.md`, `demo-surfaces.md` or
`pitfalls.md`. Its command blocks were removed. Every one of them is in
`../SKILL.md`, which is what to run, and a few were deliberately changed
in #194 (`--no-track`, the backgrounded quality gate, `branch -D` after a
`MERGED` check, removing `in-review` at step 9). Read this file when you
want to know *why* a step does something, or before editing the skill.

Step numbers refer to `../SKILL.md`, which keeps the original numbering.
The heartbeat one-liners quoted below are the pre-#276 form; they are now
`scripts/heartbeat.sh <N>`, and the claim script is `scripts/claim.sh <N>`.
"Pitfalls" means `pitfalls.md`; "Claim the issue" means `claim-protocol.md`.

This is the repo's SDLC automation: take one open GitHub issue from the
backlog all the way to merged, following `SDLC.md`'s lifecycle exactly
(issue -> feature branch -> plan review -> implement -> quality gate ->
diff review -> PR -> independent review -> merge -> cleanup). Merge
authority is whatever `SDLC.md`'s `merge_policy` frontmatter currently
says — read it fresh at step 8, don't assume from memory, since it's
the kind of thing that gets changed by re-running `sdlcify`. Expect to
revise this skill as real usage surfaces rough edges.

Argument: an optional issue number. `/work-issue 16` works that issue;
`/work-issue` with no argument picks the next un-implemented one.

## 1. Pick the issue

If an issue number was given, confirm it's open:

[command block: see ../SKILL.md]

If it's closed, stop and tell the user. Otherwise, whether it already
carries `in-progress` or not, run **Claim the issue** below for `<N>`
before doing anything else — an explicit pick can still race a
concurrent invocation targeting the exact same number, whether that's
two sessions racing a brand-new issue or one racing a crashed-or-still
-live prior run of this same one (there's no PR until step 7, so "no
PR yet" alone doesn't mean crashed — it's also the normal state of a
session still mid-run). The one case that skips the claim entirely:
it already carries `in-progress` *and* an open PR claims it (`gh pr
list --state open --json body --jq` scanning for `Closes #<N>`/`Fixes
#<N>`/`Resolves #<N>`) — that's someone genuinely already on it; stop
and tell the user, don't attempt to claim.

If no number was given, **first check for an abandoned in-progress
issue to resume** — this takes priority over picking something fresh,
otherwise a crashed run's issue gets silently skipped forever instead
of finished:

[command block: see ../SKILL.md]

For each `in-progress` issue whose number isn't in the open-PR list
(lowest first), run **Claim the issue** below. Resume the first one
that's successfully claimed (reuse its worktree if one exists — step 3
already contemplates this) instead of continuing to the picker below.
If every candidate is live (the claim fails for all of them), fall
through to the picker below rather than resuming a running session.

Otherwise, find the lowest-numbered open issue that isn't already
labeled `in-progress` and isn't already the target of an open PR:

[command block: see ../SKILL.md]

Take the first candidate not in the open-PR list, then run **Claim the
issue** below for it — even a never-before-touched issue can be raced
by two invocations picking the same lowest-numbered candidate at once.
If the claim fails, stop and report the conflict rather than falling
through to try the next candidate (deliberately not building automatic
next-candidate retry — see Pitfalls). If there are no candidates at
all, report that the backlog is clear (or everything in-flight)
instead of inventing work.

## 2. Mark it in-progress

Labels are the status signal this skill uses (not assignees — this
repo has one main contributor, and a label reads clearly in `gh issue
list` at a glance). Create the labels the first time they're needed;
`gh label create` errors if a label already exists, so don't let that
stop the run:

[command block: see ../SKILL.md]

## 3. Branch and worktree

Base the new branch on up-to-date `main` rather than whatever the
current worktree's local `main` happens to be at — the current
worktree may have uncommitted changes of its own, which is fine, since
`git worktree add` doesn't touch it.

[command block: see ../SKILL.md]

## 4. Plan, and get the plan reviewed before touching code

Read the issue as the spec: `gh issue view <N>`. This repo's issues
are written to leave the implementation approach open ("left to the
implementer") — that's intentional, not a gap to fill by asking. Make
the design calls yourself, the way AGENTS.md already asks of any
nontrivial work here: "make judgment calls on genuine coin-flip
decisions and note them; don't stall waiting for input on things
you're equipped to decide." Do flag anything that's genuinely hard to
reverse.

Write the plan out — what you're going to change, which files, the
approach and why, and anything you're deliberately *not* doing — then
call the `advisor` tool. `advisor` forwards this whole session
(everything since you picked up the issue) to a stronger reviewer with
no other context, so the plan needs to actually be written down first;
don't call it with nothing but the issue text and your intentions
still in your head. This is the point in the loop where a bad
approach is cheapest to catch — before any code exists, not after.

Take the advisor's feedback seriously: if it flags a problem with the
approach, revise the plan (and call `advisor` again if the revision is
substantial) before writing any implementation code. Don't proceed to
step 5 on a plan the advisor pushed back on without addressing why.

Refresh the heartbeat before moving on (re-derive the path and write
atomically, per step 3's note): `H="$(git rev-parse
--path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

## 5. Implement

Follow the conventions already documented for this repo while you
work — they're not restated here to avoid drifting out of sync with
the source of truth:

- AGENTS.md's "Code quality bar" (no `unwrap`/`expect` on
  program-input-derived data, comments explain why not what, tests
  live alongside the code they test).
- Commit at reasonable checkpoints, not one giant commit at the end —
  and refresh the heartbeat at each one (`H="$(git rev-parse
  --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat";
  date +%s > "$H.tmp" && mv "$H.tmp" "$H"`). Without this, step 4's end and step 5's end are
  the only two heartbeat refreshes bracketing the entire implementation
  phase — for any real code change (edit/build/test cycles, not just a
  doc tweak) that gap alone can exceed the 45-minute staleness
  threshold, making a still-live implementation look crashed to a
  concurrent invocation. Per-commit refreshes narrow that, but don't
  by themselves bound a *single* long-running stretch with no commit
  yet (e.g. one long compile or one big edit in progress) — also
  refresh right before starting the quality-gate build below, and
  again if any single step within it runs long.
- Update the notes fragment (`notes/`), `HANDOFF.md` and the matching `docs/guides/` page on the branch as
  capabilities land, per AGENTS.md — not as an afterthought.

Refresh the heartbeat before this build (it's often the single
longest-running step of the whole implementation phase):

[command block: see ../SKILL.md]

This is the same gate CI runs — clearing it locally first means the
PR isn't the first place a break shows up.

Refresh the heartbeat again once it's clean: `H="$(git rev-parse
--path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat";
date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

## 6. Get the implementation reviewed before marking done

Once the quality gate is clean, call `advisor` again — same tool, but
now it's reviewing the actual diff and decisions made while
implementing, not just the plan. This is the check before the issue
gets marked done, mirroring the plan review in step 4: catch problems
while they're still on a branch nobody's looked at yet, not after the
PR is open.

Point it explicitly at the categories the Codex review in step 8 keeps
catching that this pass tends to miss: state mutated across
control-flow branches (e.g. font/graphics-state changes mid-loop that
only bite on a later iteration), multi-byte/encoding edge cases, and
implicit side effects (a call that looks read-only but isn't). Naming
these up front costs nothing and has caught real bugs cheaper than
waiting for the several-minutes-to-an-hour Codex round trip in step 8
to find them (PR #30: a coordinate-collision bug in exactly this
category survived the full unit test suite and needed a dedicated
Codex round to catch).

If the advisor surfaces something worth fixing, fix it, re-run the
quality gate, and use your judgment on whether it's worth one more
advisor pass — don't loop indefinitely chasing a clean bill of health
on genuinely subjective feedback.

Refresh the heartbeat: `H="$(git rev-parse --path-format=absolute
--git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

## 7. Open the PR, update the issue

[command block: see ../SKILL.md]

`Closes #<N>` is what lets merging the PR close the issue automatically
— that's the only thing that should close it.

Flip the issue's status and leave a trail back to the PR:

[command block: see ../SKILL.md]

## 9. Post-merge cleanup

[command block: see ../SKILL.md]

`git worktree remove` can fail with "branch used by worktree" if run
in the wrong order — remove the worktree *before* trying to delete the
local branch, not after, or the branch delete will block on it. The
lock removal is separate and explicit — unlike the old heartbeat file,
the lock now lives in the *main repo's* shared git dir (step 1's
**Claim the issue**), not inside the worktree's own git dir, so
`git worktree remove` no longer cleans it up as a side effect.

Report back: issue number and title, branch name, PR URL and its merge
commit, and confirmation the issue closed and the worktree/branch were
cleaned up. If step 8 stopped short of merging (`human-only`, or over
the size/risk bar), report that explicitly instead — branch name,
worktree path, and the PR URL, with the worktree left in place since
there may be follow-up commits before a human merges it.


## Added in #199: the CI wait, the pending-CI hand-back, the step 4 gate

Step 8.12 used to say "if any check is pending, `gh pr checks --watch`
with `run_in_background: true`" and nothing else. The first Haiku run of
the slimmed skill (#178, PR #198) had no defined way to wait, handed
back with `test` pending, and CI passed six seconds later. So the step
now carries the exact command, and pitfalls.md "CI wait" carries the
reasoning (11-17 min CI then, about 2-3 min for `test` since #204; why no `tail`, "no checks reported", the
pending-CI hand-back checklist). The hand-back rule exists because the
skill only defined lock release for the human-only and over-the-bar
stops; a CI-pending stop needs the opposite (keep the lock and
`in-review`). Step 4.4 became a hard gate, with a report line, because
the same run skipped the plan review without noticing. Scratch files are
pinned to `/tmp` in SKILL.md's rules because the skill never named a
scratchpad but harness prompts do. (#208 reversed this: see below.)

## Added in #208: round-3 fixes

- **CI pass/fail from `statusCheckRollup`, not `exit=`.** `gh pr checks
  --watch --fail-fast` wrote `exit=0` on a run where both jobs were
  cancelled, so the exit code cannot be trusted. The rollup query guards
  `length > 0` (jq's `all` is true on `[]`, and the rollup is empty right
  after a push) and reads `.conclusion // .state` (CheckRun vs
  StatusContext).
- **`--delete-branch` dropped.** The repo already deletes the remote
  branch on merge (`delete_branch_on_merge`), and gh's local-branch half
  can never succeed while a worktree holds the branch, so it only
  produced an error. Step 9 removes the worktree, then the local branch.
- **The local gate is CI's gate** (`-D warnings`, selftest, the `ci`
  profile from #204), so a PR is not the first place a lint shows up.
- **Missing paths written down:** rerun after infra failure, resume of an
  `in-review` PR, the dispatcher's no-merge stop, doc-only merge conflicts,
  one rebase before the first push. These were improvised by the #141,
  #191 and #204 runs.
- **Rebase boundary.** Rebase once before the first push; afterwards
  merge `main` in locally (only for real conflicts) and never force-push.
  A rebase after push would need `--force-with-lease`.
- **No `BEHIND` loop (#257).** Branch protection's "require up to date"
  (`strict`) was turned off: with several agents merging, it forced an
  update-branch and a full CI rerun on nearly every PR. The cost is that
  two individually green PRs can break `main` together, so step 9 ends by
  checking the push-to-main `ci` run for the merge commit (red: stop and
  report). The merge queue is the proper fix and needs an org-owned repo.
  SKILL.md 8.12's pass/fail query adds `mergeable == "MERGEABLE"` and
  `mergeStateStatus != "DIRTY"` because a conflicted PR keeps its old green
  checks, and `UNKNOWN` (right after a push) passes `!= "DIRTY"` on its own.
- **Small hazards recorded as rules:** no `git stash` in a worktree
  (shared `refs/stash`), no `timeout` on macOS, a re-read of SKILL.md after
  step 3 (the worktree copy may be newer than the invoked one), and a short
  resume table with a conservative default (re-review) after one run spent
  eight Codex rounds on an over-detailed one.
- **Scratch = harness scratchpad, else `${TMPDIR:-/tmp}`.** #205's `/tmp`
  override fought the harness's own instruction; issue-scoped names keep
  concurrent runs apart, and resume decisions use GitHub state because
  a scratchpad does not outlive its session.

## Added in #273: parallel-agent friction

- **Notes fragments.** Every PR prepended a section to `NOTES.md`, so any two
  open PRs conflicted there (most DIRTY PRs on 2026-10-13 were NOTES-only).
  New entries go in `notes/<issue>-<slug>.md`. No regenerated index: it would
  be a shared file again. `NOTES.md` stays as frozen history.
- **`ci.yml` stops cancelling push runs.** The per-ref `cancel-in-progress`
  group cancelled each merge commit's run on the next merge, leaving agents
  to hunt for a later run. PR runs still cancel (their comment race is the
  reason the setting exists). Push runs are keyed by SHA, because with a shared
  group and cancel off GitHub still drops a pending run when a newer one
  queues.
- **Main-CI wait loop** instead of listing once and guessing; rerun only for a
  zero-step infrastructure failure with no newer push.
- **8.12 query** requires `mergeable == "MERGEABLE"`; a `git merge-tree` check
  runs before the CI wait so a PR that will go DIRTY is caught first.

## Round 7 (#303): why the rebase moved late and the merge re-queries

Both come from runs that were correct when they checked and stale when they merged. The identity
check, the gate and a committed specimen all describe one tree; a rebase after any of them
(pulling in `lib/`, `src/` or tests) silently makes the result describe a tree that no longer
exists. So the rebase is the last thing before those checks, and the pitfalls matrix says what
to redo when it pulls in code versus docs only. The merge-state query reads GitHub's lazily
computed `mergeable`/`mergeStateStatus`, which can print `false` or `UNKNOWN` for a few seconds
after a watch ends even with every check green, so it is repeated right before `gh pr merge`.
Pattern kills (`pkill -f`) are forbidden outright because every agent on the machine shares one
process table: only a PID you recorded from `$!` is yours.

## Round 8 (#331): a way out without a PR, prototypes, and a no-merge hand-over that is green

The skill assumed every run ends in a PR. A run whose measurement disproves the issue and leaves
nothing worth committing had no exit: finish.md's "Close without a PR" is that exit (the
`Refs` won't-change PR stays for findings worth shipping as notes). The plan gate says no code
before the advisor call; a throwaway prototype in a scratch worktree is code that is never
shipped, so it is allowed after the gate, cleaned up by exact path. A no-merge dispatch used to
stop before 8.12, which left the maintainer a PR with CI possibly red or running; it now waits for
green and stops before 8.13 only. Timing advice has one home (pitfalls "Prototypes and timing"),
points at `bench_compare.sh` and the AGENTS.md CPU-time convention, and stays advisory. The
pitfalls contents list is by section name because line numbers drifted on every edit.
