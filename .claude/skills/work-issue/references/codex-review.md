# Step 8 in full: independent review, then merge

Read by `SKILL.md` step 8. `SKILL.md` carries the exact commands in the
order to run them; this file is the complete original step 8 — policy,
rationale, output format, posting templates, fallback path, merge
eligibility and the pre-merge summary format. Where the two differ,
`SKILL.md`'s commands win only for the deliberate changes listed in the
PR that split this skill (#194) and in #208 (no `--delete-branch`, the
`statusCheckRollup` pass/fail rule, `<SCRATCH>` paths, the no-merge stop);
every guard below still applies.

Step numbers ("step 1" ... "step 9") refer to the numbered steps in `../SKILL.md`, which keep their original numbering. "Pitfalls" / "the ... pitfall below" means `pitfalls.md` in this directory; "Claim the issue" means `claim-protocol.md`.

## 8. Independent review, then merge per `SDLC.md`'s policy

Read `SDLC.md`'s frontmatter fresh (`merge_policy.mode` and, if
`size-and-risk-bar`, `max_changed_lines`/`sensitive_paths`) — this is
config, not something to remember from a prior run or from having
written the skill.

**If this branch's diff touches
`.claude/skills/work-issue/SKILL.md` itself**, note that explicitly in
the PR/issue comments posted below: this session loaded `SKILL.md`
before the change existed, so this very step-8 pass is running the
*old* review logic even while reviewing a diff that may have changed
it (confirmed happening for real in #26/#27 — the PR that added Codex
review wasn't itself reviewed by Codex, because the fallback path was
all the session had loaded). A human reading the comment then knows to
re-check step 8's behavior against what actually merged, rather than
trusting this run's self-report of "clean."

**Review must come from a different model family than whichever one
implemented the change.** This skill only ever runs as Claude, so its
review dispatches to Codex — never a same-family blank-context `Agent`
review, which shares training and blind spots with the implementer no
matter how little context it's given. (Symmetric rule, stated once:
if a Codex-driven equivalent of this skill is ever built, its step 8
dispatches to a blank-context Claude `Agent` instead — no runtime
detection needed, since each skill only ever runs as the model it's
written for.)

Locate the Codex plugin's runtime script rather than assuming
`$CLAUDE_PLUGIN_ROOT` — that env var is populated for plugin-owned
commands, not project skills like this one, so it may be unset or
point somewhere unrelated:

```sh
CODEX_SCRIPT=$(find "$HOME/.claude/plugins" -path "*/codex/scripts/codex-companion.mjs" 2>/dev/null | head -1)
```

If nothing is found, or `node "$CODEX_SCRIPT" status --json` shows the
runtime isn't authenticated (run the `codex:setup` skill to check),
don't silently skip cross-model review — fall back to a blank-context
Claude `Agent` review (self-contained prompt: nothing but the PR
number and what to check; it fetches `gh pr view`/`gh pr diff` itself,
it doesn't inherit this session's view of them) and say explicitly in
the PR/issue comment (below) that this was a same-family fallback, not
policy, so a human knows to expect a real Codex pass once the runtime
is fixed.

Otherwise, run the Codex review from inside `$WORKTREE_DIR` — cwd can
reset between tool calls (see Pitfalls), and a review run from the
wrong directory reviews the wrong tree and still returns a confident
verdict, so assert the branch before trusting the output. Refresh the
heartbeat right before kicking it off — this is the longest-blocking
step (minutes to tens of minutes per round) and the one most likely to
make a still-live run look stale to a concurrent invocation.

**Run this as a normal blocking Bash call — never pass
`run_in_background: true` on it, and never call `ScheduleWakeup` in
the same turn that launches it.** That exact pairing has twice
produced a review that looked like it finished when it hadn't: once a
0-byte `<SCRATCH>/codex-review-<N>.json` behind a task notification that
still said `completed`, once the process reported outright `killed`
(#57, not root-caused — observed only on turns combining the two, not
on plain foreground runs). Let the call block; if the harness
auto-backgrounds it anyway past its own timeout, see the matching
Pitfall below for how to check on it without `ScheduleWakeup`:

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

`rm -f` stays first in the chain, exactly as it originally was — if any
later step in the chain fails (heartbeat write, branch assert,
`$WORKTREE_DIR`/`$BRANCH` empty from a cwd/variable reset), the stale
`<SCRATCH>/codex-review-<N>.json` from a *prior* round is already gone, so
the "before posting anything" check below can't accept and post that
prior round's output as if it were a review of the current diff — a
short-circuit that isn't `rm -f`-first would silently do exactly that
under `agent-full` merge authority.

`rm -f` first so a short-circuited chain (the branch assert failing, or
`$WORKTREE_DIR`/`$BRANCH`/`$CODEX_SCRIPT` coming up empty from a
cwd/variable reset — see Pitfalls) leaves *no* file behind rather than
leaving a prior round's stale review sitting at that path where the
posting step below would read and post it as if it were current.

The review model is pinned explicitly with `--model gpt-6.1-sol` rather
than inherited from `~/.codex/config.toml`, which can be stale or differ
per machine. The companion's `review` subcommand accepts no `--effort`
flag (only `task` does), so reasoning effort (`high`) still comes from
`model_reasoning_effort` in `~/.codex/config.toml`; it cannot be pinned
from here.

If `gpt-6.1-sol` (or any newer pin) is rejected with "not supported when
using Codex with a ChatGPT account", the account may just not have it
yet — but first rule out a stale login: after `codex login`, the
session's already-running shared broker keeps the *old* credentials, so
restart it (see the stale-broker Pitfall) and retry before concluding the
model is unavailable. Test a new pin with a one-line
`task --fresh --model <m> "Reply with exactly: pong. Make no edits."`
before editing this step.

This blocks (expect several minutes — it's a real model pass, it reads
around the diff for context) and, on success, writes a JSON object
shaped `{review, target, codex: {status, stderr, stdout, reasoning}}`
— **not** `review-output.schema.json`'s `{verdict, findings[], ...}`;
that schema belongs to a different internal mode and does not describe
this command's actual output (confirmed empirically — an earlier draft
of this skill assumed it did, and `jq`-ing for `.verdict`/`.findings`
silently returned `null` and would have posted nothing useful). The
real content is `.codex.stdout`: free-form markdown — a short summary
paragraph, then a "Full review comments:" section with bullets each
tagged by priority (`[P1]`, `[P2]`, ...) inline in the prose, not
structured fields. The mapping is `P0`=critical, `P1`=high,
`P2`=medium, `P3`=low — stated here so it doesn't need re-deriving by
reasoning every run. Treat the tagging itself as a rough heuristic,
not a contract — it's model-generated text, not a schema, and its
exact shape can drift between runs.

**Read the whole thing yourself and decide what needs fixing —
don't gate merge on a `[P1]`-only regex.** A finding describing a real
defect must be fixed or given an explicit stated reason it's not being
fixed ("not a bug", "out of scope, tracked separately"), regardless of
which priority tag Codex put on it; an unexplained skip isn't
acceptable — `SDLC.md`'s independent-review step requires coming back
*clean*, not merely *reviewed*, and priority tags from a free-text
reviewer aren't reliable enough to auto-waive anything. Use `[P1]` (or
worse) as a signal for "definitely stop and look," not as the entire
policy.

**Before posting anything, check whether the run actually succeeded.**
If `.codex.status` is non-zero, or `.codex.stdout` is empty/null, the
run failed rather than approved — don't post it as if it were a clean
review; treat it the same as "Codex runtime unavailable" below (skip
straight to the Fallback path, don't post from this file at all).

```sh
jq -e '.codex.status == 0 and (.codex.stdout // "" | length > 0)' <SCRATCH>/codex-review-<N>.json
```

Only once that passes: **post the review on both the PR and the
issue, unconditionally — including a clean pass with nothing to
fix.** This is the actual point of cross-model review: a durable
record a human can read without re-running anything, not just a
merge/no-merge gate.

```sh
gh pr comment <PR#> --body "$(jq -r '"## Codex review\n\n" + .codex.stdout' <SCRATCH>/codex-review-<N>.json)"
```

**The issue gets the findings themselves, not a one-line pointer.**
The PR comment above is verbatim; the ticket must also carry every
finding and what was done about it, because the issue is where a human
reads the history and `<SCRATCH>/codex-review-<N>.json` is overwritten by the
next round (#160 — on #147 only the PR held the verbatim text). Once
you have read the review and decided what to do about each finding,
post one comment per review round on the **issue** (a fresh comment
each round; never edit or replace an earlier round's):

```sh
gh issue comment <N> --body-file - <<'EOF'
## Codex review, round <R> (PR <PR URL>, reviewed head <full sha of `git rev-parse HEAD`>)

<details><summary>Codex output, verbatim</summary>

<paste .codex.stdout exactly as returned>

</details>

| # | Finding (Codex's words, with its [P#] tag) | Real defect? | Our response |
|---|---|---|---|
| 1 | ... | yes / no / partly | Fixed in <short sha>: <what changed> — or — Not fixing: <explicit reason> |
EOF
```

The `reviewed head` in the heading (the commit `--scope branch` actually
reviewed) is what a resumed run compares the PR head against; without it a
resume cannot tell a current review from a stale one (`pitfalls.md`
"Resume paths", Codex round 4 on #208). The `## Fallback review, round <R>`
heading carries it too.

Every finding gets a row, including ones judged not real (say why) and
including a clean pass ("no findings", one row or a sentence). "Our
response" is written after the fix is committed, so it can cite the
commit. If a round produced fixes, the next round's comment records the
re-review result the same way.

If anything needs fixing: fix it, **commit and `git push` the fix**,
then re-run the quality gate and re-run the Codex review on the
updated diff (repeat the whole block above, including posting the new
review as a fresh comment — don't overwrite the prior one) before
continuing. Pushing before re-reviewing matters here specifically:
`--scope branch` diffs local git state, so a fix that's only committed
locally makes the *next local review* look clean while the actual PR
on GitHub — what CI tests and what merge would actually ship — still
points at the old, unfixed commit. Use your judgment on how many
rounds are worth it on genuinely subjective feedback.

Once the review is clean — nothing left unfixed without a stated
reason — proceed to merge-eligibility below.

**Fallback path** (Codex runtime unavailable/unauthenticated, or
`.codex.status` non-zero / empty output above): use the `Agent` tool
with a self-contained prompt that gives it nothing but the PR number
and what to check (title/body via `gh pr view`, diff via `gh pr diff`
— the agent fetches these itself, it doesn't inherit this session's
view of them). Post its review to the PR and issue under the same
policy as above — unconditionally, including a clean result — but
via `gh pr comment`/`gh issue comment` with the agent's own report
text directly as the body; there's no `<SCRATCH>/codex-review-<N>.json` in
this path, so the `jq` mechanics above don't apply. Note in both
comments that this was a same-family fallback *and why* (the actual
error text and what you tried; see the stale-Codex-broker Pitfall
before concluding Codex is really unavailable). The issue comment uses
the same per-finding format as above, headed `## Fallback review, round <R>` (verbatim report + disposition
table). Same rule as above: fix or explicitly disposition everything it
raises, commit and push before any re-review, before continuing.

With the review clean (nothing left unfixed without a stated reason,
via whichever path ran), decide merge eligibility from the policy:
- `human-only`: stop here. Release the lock (`rm -rf "$(git rev-parse
  --path-format=absolute --git-common-dir)/work-issue-lock-<N>"`) —
  the label already flipped to `in-review` at step 7, so step 1 can't
  re-select this issue via the automatic picker regardless, but
  releasing explicitly here still matters for the explicit-number path
  (`/work-issue <N>` re-run after a human closes this PR unmerged and
  leaves the issue open, for instance). Report the PR as open and
  awaiting merge; don't attempt to merge it.
- `agent-full`: eligible regardless of diff size or files touched, unless
  the dispatcher said not to merge: post the summary below, release the
  lock, keep `in-review`, the worktree and the branch, and report
  `awaiting human merge (dispatcher said no merge)`. `pitfalls.md`
  "Stop without merging" and "Resume paths" have the stop and the
  resume-to-merge route (#208).
- `size-and-risk-bar`: eligible only if the diff (excluding doc-only
  files) is under `max_changed_lines` and touches none of
  `sensitive_paths`; otherwise stop here (same lock release as the
  `human-only` case above) and say which condition it missed.

**Before merging, post the summary on the issue** (#160). The PR's
`Closes #<N>` closes the issue the instant it merges, so the
wrap-up has to already be there — a comment added afterwards lands on a
closed ticket nobody is watching. Write it from what actually happened,
not from the plan. Start it with the exact heading
`## Summary of work (PR #<PR>)` (step 9 greps for it). It has these parts:

- **Delivered** — files/artifacts and the exact render/run commands.
- **Decisions** — each judgement call and why (including any the user
  made in conversation, and any second opinions asked for and what they
  said, even when they disagreed with the outcome).
- **Review record** — every round, which reviewer (Codex, or the
  same-family fallback and why), and a pointer to the per-round
  finding/response comments above; list defects found and how each was
  fixed.
- **Known deviations / open points** — what still differs from the spec
  or the original, and any follow-up worth filing.

```sh
gh issue comment <N> --body-file <SCRATCH>/issue-<N>-summary.md
```

Skip this only when the merge policy stops before merging
(`human-only` / over the bar); then post it when the human merges, or
say in the step-9 report that it is still owed.

If eligible, merge:

```sh
gh pr view <PR#> --json mergeable,mergeStateStatus,statusCheckRollup
```

If `mergeStateStatus` is `BEHIND` (branch protection's `strict` status
check requires the tested commit to reflect current `main` — a PR
opened a while ago, or after another PR merged first, will hit this),
update and wait for CI before merging (with several agents merging,
`BEHIND` is the normal case, not an exception, #208):

```sh
gh pr update-branch <PR#>
# then wait with SKILL.md step 8.12's background recipe (pitfalls.md "CI wait"); never poll or sleep.
# Pass/fail comes from the statusCheckRollup query there, not from the watch's exit= line.
```

Then merge using the shape recorded in `SDLC.md`'s `merge_button`
block (this repo: squash; `delete_branch_on_merge: true` makes GitHub
delete the remote branch itself):

```sh
gh pr merge <PR#> --squash
```

No `--delete-branch` (#208): gh also tries to delete the *local* branch
and switch the main checkout, which fails while the branch is checked out
in the worktree, so it printed a harmless error on every run. Step 9
removes the worktree, then the local branch.

If CI fails on the updated branch, don't force past it — treat that
as a blocker the same as a failed review, and report it rather than
merging anyway. Cancelled or never-started jobs (infrastructure) may be
re-run once the `pitfalls.md` "CI wait" check shows no step ran; a real
failed step is a blocker.
