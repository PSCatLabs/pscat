## SDLC: lessons from the dispatcher retrospective (issue #674, 2026-10-21)

Applies the owner-approved decisions from the retrospective of the October
dispatcher run. Process and docs only, plus two small tools; no render changes.

**What changed**
- `SDLC.md` "Who merges" is now four tiers: (a) no visible change, the agent
  merges; (b) a visible render change, the agent merges after an independent
  vision-capable reviewer agent passes the before/after images against the
  issue's perceptual check and `docs/dev/TASTE.md`, and logs it in
  `docs/dev/review-log/pending/` for the owner's read before the release;
  (c) CODEOWNERS paths, the owner merges, batched, after green CI with a
  Decision block atop the PR; (d) a relayed approval never authorizes an agent
  merge. Driven by new frontmatter (`merge_policy.visible_change_review`,
  `light_gate`, `dispatch_doc`) that the sdlcify skill and its template now
  render, so a re-run reproduces it.
- work-issue: a goals preamble (quality first, performance as a guard, no gs
  timing, wait for CI), `references/perceptual-review.md` (tiers, the
  reviewer prompt, the log fragment, the Decision block), proposed follow-ups
  in the final report instead of `gh issue create`, the light gate for
  docs-only diffs, papercuts and render DPIs in `pitfalls.md`.
  `SKILL.md` stays at 250 lines.
- `scripts/identity_check.sh --expect-diff LIST`: listed files may differ
  (`EXPECTED`), a listed file that comes out identical fails (`UNCHANGED`), a
  listed file is compared even when the pick leaves it out, and one that is new
  in the branch (no base render) is a usage error.
- `scripts/pr_body_lint.py` and the `pr-body` workflow: a closing keyword in a
  negated or partial sentence ("does not fix #N") fails. Run over the last 200
  PR bodies it flags one, and it is real (PR #438, whose merge closed an issue it said it left open).
- Templates: a Decision block and `Refs #N` guidance in the PR template; "Why /
  design intent" and "Perceptual acceptance check" in the feature request
  form (the dispatcher's `gh issue create` bodies carry the same headings,
  `docs/dev/DISPATCH.md`); a review-log step in the release checklist.

**Decisions**
- Review-log fragments are named by issue number, like `notes/`, so parallel
  agents never touch the same file and the name is known before the PR exists.
- The pr-body lint is its own workflow so a body edit re-runs only it, not the
  Rust suite. It is not a required check.
- The dispatcher's handoff file is `.dispatch/handoff.md`, git-ignored, with a
  copy posted on the wave's epic.
- `docs/dev/TASTE.md` is a CODEOWNERS path: it is tier (b)'s merge criterion.

**Deferred**
- `SDLC.md` sections this PR did not touch still differ from the sdlcify
  template's wording (lifecycle step 5's CI reference, the branch-protection
  prose, the release steps, "Labels"). A full sdlcify re-run would rewrite
  them; left alone to keep this diff to the policy change.
