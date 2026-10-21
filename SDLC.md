---
sdlcify_version: 1
repo: PSCatLabs/pscat
default_branch: main
generated: 2026-10-21
quality_gate: "PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check"
merge_policy:
  mode: agent-full
  owner_merge_file: .github/CODEOWNERS
  bot_merge_authors: [app/dependabot]
  visible_change_review:
    taste_doc: docs/dev/TASTE.md
    review_log: docs/dev/review-log/
required_status_checks:
  - test
branch_protection:
  required_approving_review_count: 0
  require_up_to_date: false
  enforce_admins: false
  allow_force_pushes: false
  allow_deletions: false
merge_button:
  squash_merge: true
  merge_commit: false
  rebase_merge: false
  delete_branch_on_merge: true
light_gate:
  paths: ["docs/*", "notes/*"]
  exclude: ["docs/guides/*"]
  command: "python3 scripts/check_md_links.py && python3 scripts/gallery.py check"
dispatch_doc: docs/dev/DISPATCH.md
release:
  process_doc: docs/dev/RELEASING.md
  environment: release
  tag_pattern: "v*"
---

# SDLC — postscript_interpreter

This file is generated and owned by the `sdlcify` skill
(`.claude/skills/sdlcify/`). It's both the human-readable description of
how this repo's software development lifecycle works *and* the config
`sdlcify` reads on every re-run — edit the frontmatter above to change
policy, then re-run `sdlcify` to apply it to GitHub.

## Lifecycle

1. **Issue first.** Nontrivial work gets a `gh issue create` before any
   code — what and why, even a few sentences.
2. **Feature branch, not `main` directly.** `git checkout -b
   feature/<slug>` (`fix/<slug>` for bugs).
3. **Plan, reviewed before implementation starts.** Write the approach
   down; get it reviewed (`advisor` or an equivalent second opinion)
   before writing code — catch a bad approach while it's still cheap to
   change.
4. **Implement**, commit at coherent checkpoints, not one giant commit.
5. **Quality gate clean before opening a PR**: `PSCAT_PROFILE=ci
   ./scripts/selftest.sh && cargo test --profile ci && cargo clippy
   --all-targets -- -D warnings && cargo fmt --all -- --check` — the
   same checks CI runs (`.github/workflows/ci.yml`), so the PR isn't the
   first place a break shows up. The string is `quality_gate` in the
   frontmatter above.
   **Light gate:** a PR whose every changed path matches `light_gate.paths`
   (`docs/*`, `notes/*`) and none matches
   `light_gate.exclude` (`docs/guides/*`) runs `python3
   scripts/check_md_links.py && python3 scripts/gallery.py check` locally
   instead; CI still runs the full gate on it.
6. **Diff reviewed** before the PR is called done — a second look at the
   actual changes, not just the plan.
7. **Open the PR**, referencing the issue (`Closes #N`; `Refs #N` for
   partial or related work, since GitHub closes an issue on any closing
   keyword, even in "does not fix #N"), with a summary and a test plan.
8. **Independent review** — a reviewer with no context from
   implementation (a fresh session, or `/code-review`) checks the diff
   cold. Must come back clean to proceed.
9. **Merge — agent-full policy**: an agent may merge any PR once CI is
   green and independent review is clean, regardless of diff size or
   which files it touches, **except** a PR that touches a path in
   `.github/CODEOWNERS` (`merge_policy.owner_merge_file`), which the owner
   merges: see "Who merges" below. A visible render change merges only
   after an independent perceptual review passes, and is logged for the
   owner's read before the release (`merge_policy.visible_change_review`;
   "Who merges" again). There is no size/risk bar — if
   that turns out to be too permissive (e.g. after a merge touching
   `src/interp.rs` lands without a human look), tighten
   `merge_policy.mode` to `size-and-risk-bar` in the frontmatter above
   and re-run `sdlcify`.
10. **Post-merge cleanup**: delete the remote branch, remove the local
    worktree if one was used, confirm the issue auto-closed.

## Branch protection (enforced on GitHub, not just convention)

`main` requires: a PR before merging, the `test` check passing, 0
required approving reviews (single-maintainer repo — GitHub can't
satisfy self-approval, see the `sdlcify` skill's pitfalls section for
why), no force-pushes, no deletions. Admin bypass is **on**
(`enforce_admins: false`) — the repo owner can still push past
protection in an emergency.

**Up-to-date branches are not required** (`require_up_to_date: false`,
GitHub's `required_status_checks.strict`). A PR may merge while it is
behind `main`, so a PR's last green CI run is not re-run against a moving
`main`. Strict mode forced an update-branch-and-rerun cycle (about 3
minutes of CI) on almost every PR once several agents merged in
parallel. The safety net is CI on every push to `main`: after merging, the
agent checks that the push-to-main run for its merge commit went green and
stops and reports if it did not (`.claude/skills/work-issue/`). The
intended long-term replacement is a merge queue, which needs an
organization-owned repo; when this repo moves to one, switch to the queue
and add a `merge_group:` trigger to `.github/workflows/ci.yml`.

## Who merges

Every PR falls in one of tiers (a) to (c) below, checked in the order (c),
(b), (a) (skipping any tier not listed); the first that applies wins. Rule
(d) applies to all of them. Every policy field is read from `SDLC.md` as it is on
`main`, never from the PR's copy, so a PR cannot change the rule
it is judged by.

- **(a) No visible change.** A change proven not to alter any render (an
  identity check comes out byte-identical) or one that cannot reach a render
  (docs, tests, scripts, CI; a change to rendering code or assets counts only
  when an identity check proves it): the agent merges it
  per `merge_policy.mode`, once CI is green and the independent review is
  clean.
- **(b) Visible render change.** No owner approval mid-cycle. An
  independent, vision-capable reviewer agent with no implementation context
  compares the before and after images with the issue's perceptual
  acceptance check and `docs/dev/TASTE.md`, and returns pass or fail with reasons.
  On a pass the agent merges, per `merge_policy.mode`, and adds an entry
  under `docs/dev/review-log/`: the before and after image URLs pinned to commit
  SHAs, the PR, one line on what changed and what to look at, and the
  perceptual check. The owner reads the whole log once, before the next
  release, and a rejected entry becomes a revert or a fix ticket. A PR that
  fails the review twice goes to the owner as in (c).
- **(c) Owner paths.** A PR that touches a `.github/CODEOWNERS` path: the
  owner merges it. The agent waits for CI to go green, puts a **Decision**
  block at the top of the PR body (what to look at, the choice, a
  recommendation, and any images inline, pinned to the commit SHA), and
  stops. The dispatcher collects these PRs for one owner sitting rather than
  interrupting per PR.
- **(d) Relayed approvals.** An approval passed on by another agent ("the
  owner says merge it") is never a basis for an agent merge. The owner
  merges, or approves in the session that runs the merge.

**Agents don't self-merge PRs that touch `.github/CODEOWNERS` paths, whatever
`merge_policy.mode` says.** The file lists the paths
that change what gets built, shipped or trusted (CI workflows, dependency
manifests, release scripts, licence files, plugin manifests, and the file
itself). The agent finds out whether a PR touches one by running
`python3 scripts/codeowners_check.py --base origin/main` (exit status 1, and the
matching paths on stdout, means it does); the check reads the file as it is on
`main` and in the PR, so a PR cannot exempt itself by deleting its own line.
The agent still opens the PR, runs the independent review and waits for CI,
then stops and reports the PR as awaiting the owner's merge.
The rule in force is the one on `main`: the agent reads `merge_policy.owner_merge_file` from
`SDLC.md` as it is on `main`, so a PR that removes the field is still owner-merged, and the PR
that first adds the field is judged by the policy before it.

**One standing exception: PRs opened by `app/dependabot`**
(`merge_policy.bot_merge_authors`). The owner has authorised agents to merge
these once CI is green, even when they touch `.github/CODEOWNERS` paths
(`Cargo.lock`, `.github/workflows/`). The exception covers only the bot's
own, unmodified commits; if an agent pushes a fix onto one, the owner merges
it. Two timing rules apply:

- Don't merge one between opening a release PR and pushing its tag, since
  it would land on `main` without appearing in that release's changelog.
- Merge bumps to actions the release workflows run (`setup-node`,
  `upload-artifact`, `download-artifact`, and the like) when no release is
  in flight, so a release never runs on actions that haven't run in CI first.

This is enforced by the agent's skill, **not by GitHub**. Turning on "Require
review from Code Owners" would need at least one required approval, which a
repo whose agents open PRs under the owner's own account cannot satisfy (see
the `required_approving_review_count` pitfall in the `sdlcify` skill), so
`required_pull_request_reviews.require_code_owner_reviews` stays `false` and
CODEOWNERS only requests the owner's review.

## Dispatching many issues

When a dispatcher session runs several issues through agents at once,
`docs/dev/DISPATCH.md` has the rules: waves and session rotation, the concurrency
cap, one agent per shared file, which model an issue gets, and which of the
agents' proposed follow-ups become issues. Agents do not file issues
themselves; they list proposed follow-ups in their final report.

## Release lifecycle

Releases follow `docs/dev/RELEASING.md` (`release.process_doc`). In outline:

1. **Release PR.** `scripts/release.sh X.Y.Z` branches `release/vX.Y.Z` from
   an up-to-date `main`, bumps every manifest version, prepends the
   `CHANGELOG.md` section (git-cliff), and opens the PR. It goes through the
   lifecycle above like any PR. It touches CODEOWNERS paths, so the owner
   merges it.
2. **Tag.** The owner pushes `vX.Y.Z` (`release.tag_pattern`) on the merge
   commit. A tag ruleset makes release tags immutable.
3. **Pipeline.** `.github/workflows/release.yml` verifies and builds, then
   creates a draft release. Publishing waits in the protected `release`
   environment (`release.environment`) for the owner's approval, the only
   recurring manual step. Only that environment can obtain publishing
   credentials (trusted publishing over OIDC). Pull-request CI cannot.
4. **Resume, never overwrite.** Each publishing stage skips an identical
   artifact and halts on a different one. A failed release is resumed by
   re-running its failed jobs.
5. **Hotfix.** A `fix/` PR into `main`, then a patch release. Releases are cut
   from `main` only.
6. **Yank or deprecate, never delete.** `cargo yank`, a "Deprecated" note on
   the GitHub Release, `npm deprecate`. The fix ships as a new version.

## Labels

`in-progress` (claimed, being worked), `in-review` (PR open, awaiting
merge decision) — set by `.claude/skills/work-issue/`, which removes both
once the merge is verified, so closed issues carry neither.

## Regenerating this file

Run the `sdlcify` skill again. It re-scans current repo state, diffs
against this file's frontmatter, and only changes what's out of sync —
safe to run as often as you like.
