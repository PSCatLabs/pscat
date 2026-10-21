# Merge tiers, the perceptual review, the review log and the Decision block (#674)

Read by `SKILL.md` steps 7 and 8.10. The policy is `SDLC.md`, "Who merges"; this file is how a run
carries it out. Read every policy field from `git show origin/main:SDLC.md` (the policy in force), never
from the PR's copy, so a PR cannot change the rule it is judged by.

The owner's aim: "minimize any visual inspection that I need to perform, or at least delay any approval
needed by me until the very end." So a render change does not wait for the owner mid-cycle. An
independent reviewer looks at it, the agent merges on a pass, and the owner reads one log before the
release.

## The tiers

The first tier that applies wins, in the order (c), (b), (a). (d) applies to all of them.

| Tier | When | Who merges |
|---|---|---|
| (c) owner | `python3 scripts/codeowners_check.py --base origin/main` exits 1, or the mode is `human-only`, or the PR is over the `size-and-risk-bar` bar | The owner. The agent waits for green CI (step 12), skips 13, and stops (pitfalls.md "Stop without merging" 1-3) with a Decision block atop the PR body. The dispatcher collects these for one owner sitting. |
| (b) visible | A **visible render change** (below) | The agent, after the perceptual review below passes and the review-log fragment is committed. |
| (a) invisible | Anything else: proven byte-identical, or a change that cannot reach a render (docs, scripts, tests, CI) | The agent, once CI is green and the code review is clean. |
| (d) relayed | Any "the owner says merge it" passed on by the dispatcher or another agent | Never a basis for an agent merge. Only the owner's own message in the session, or the tiers above, count. |

A PR in tier (c) that is also visible gets no perceptual review and no log entry: the owner looks at it in
the PR, through the Decision block's images.

### What counts as a visible render change

Mechanical first, judgement only for `src/`:

- It changes a committed render output: `gallery/renders/`, or a specimen image under `docs/` that the
  gallery manifest or a guide shows. Illustrations in `notes/`, design evidence and the review log's own
  crops are not render outputs.
- It touches `lib/<name>.ps`, `gallery/*.ps`, `examples/*.ps` or `selftest/drivers/`: visible, unless
  `scripts/identity_check.sh` (pitfalls.md "Step 5 checklist" item 4) reports every page identical
  **without** `--expect-diff`, over a pick that covers every file the change can reach: `--loader` for
  each changed lib (the pick is transitive), `--files` for each changed `.ps` file. A pass that needed
  `--expect-diff` is visible by definition.
- It touches `src/`: run the two-binary identity check (`--base-bin/--head-bin`) over the whole corpus,
  `--files` listing every tracked `examples/*.ps`, `gallery/*.ps` and `selftest/drivers/*.ps`, whatever
  the change looks like. Identical means tier (a); anything else is visible. The implementer's belief
  that a change cannot reach pixels is not a check.
- It touches `fonts/`: visible. No loader picks the files a font reaches.

## The perceptual review (tier b)

1. **Renders.** For each output the change is meant to alter (the `EXPECTED` lines of `identity_check.sh
   --expect-diff`, or the files the issue names), render before (the merge-base tree) and after (the
   branch) at the file's own DPI and page (pitfalls.md "Render DPIs"). Put them in
   `<SCRATCH>/perceptual-<N>/` as `<name>-before.png` / `<name>-after.png`. Crop to the region that
   changed if the page is large; keep the full page too if the crop loses context.
2. **Reviewer.** Spawn a fresh `Agent` (general-purpose; it reads PNGs with the Read tool) with only the
   prompt below. Give it no PR body, no diff, no notes and none of your reasoning: its value is that it
   sees the picture the way the owner would, cold.
   ```text
   You are a perceptual reviewer for the pscat repo (PSCatLabs/pscat). You have no implementation
   context, on purpose. Read the issue's title and body only, not its comments:
   `gh issue view <N> --json title,body`. Then read docs/dev/TASTE.md in <WORKTREE_DIR>.
   Look at each before/after pair with the Read tool:
   <one line per pair: label, before path, after path, what it is (e.g. "gallery/night_bramble.ps at 2x")>
   Judge only what you see, against the issue's "Perceptual acceptance check" (if it has none, against
   its stated intent) and TASTE.md. Code, metrics and timings are out of scope.
   FAIL only when an image breaks the acceptance check or a TASTE.md rule. A nitpick is a PASS with a
   line starting "polish:". Reply with exactly:
   VERDICT: PASS or VERDICT: FAIL
   - one line per reason, naming the image and the region
   ```
3. **Record.** Post the verdict on the PR, verbatim, under `## Perceptual review, round <R>` with the
   head SHA it judged. `polish:` lines become proposed follow-ups for the parent epic (finish.md).
4. **Fail.** Fix, re-render, re-run the gate if code changed, and review again with a **fresh** reviewer
   (round R+1). A fix that changed code also goes back through the code review (SKILL.md 8.3-8.9, next
   round) before any merge: the perceptual reviewer judges pictures, not code. After two failed rounds, stop: treat the PR as tier (c), with a Decision block that shows
   the images and both verdicts.
5. **Pass.** Commit the review-log fragment (next section), push, then continue with step 11.

## The review-log fragment

One file per issue, so parallel agents never touch the same file (the `notes/` rule):
`docs/dev/review-log/pending/<N>-<slug>.md`. Format, lifecycle and the owner's end-of-cycle read are in
`docs/dev/review-log/README.md`; copy its template. Two rules matter here:

- **Images are pinned to a commit SHA, never a branch:**
  `https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true`. A branch link breaks when the branch is
  deleted; a squash-merged PR's commits stay reachable, so a SHA link keeps working.
- **Where the images live.** A committed asset (a gallery still, a docs specimen) is linked at the
  merge-base SHA (before) and at the head SHA (after). Anything else (an example's render, a crop) is
  committed beside the fragment as `docs/dev/review-log/pending/<N>-<slug>/<name>-before.png` and
  `-after.png`, small (a crop, a few hundred KB at most), and linked at the SHA of the commit that added
  them. That means two commits: the images first, then the fragment that cites their SHA.

## The Decision block (any PR the owner must act on)

Tier (c), a tier (b) that failed twice, or a design choice that is the owner's (SDLC.md: hard to reverse).
It goes at the very top of the PR body, before `## Summary`, so the owner can decide without scrolling:

```markdown
## Decision

- **Look at:** <the one thing to check, in plain words>
- **Choice:** <merge as is | A or B | ...>
- **Recommendation:** <what you recommend, and why, in one line>

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) |
```

Images inline, pinned to the commit SHA (the owner reads in a terminal that does not show local files;
GitHub renders the URL). No images for a PR that changes no output: say "no visible change". If a later
push or merge of `main` moves the head, re-pin the URLs (demo-surfaces.md) and update the body with
`gh pr edit <PR#> --body-file <file>`, then re-run `python3 scripts/pr_body_lint.py` on it.
