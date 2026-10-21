# Dispatching: running many issues with agents

How a dispatcher session (the main session, driving one `/work-issue <N>` sub-agent per issue) runs
the backlog (#674). `SDLC.md` is the policy each agent follows; `.claude/skills/work-issue/` is how one
agent ships one issue. This page is the layer above: waves, lanes and what the dispatcher does with
what the agents hand back.

## Waves and session rotation

- **A wave is one epic or about 10 tickets, whichever comes first.** Long sessions accumulate stale
  context and drift from the skill's current text; a fresh session starts clean.
- **After each wave the dispatcher writes a short handoff file and the next wave starts in a fresh
  session.** The file is `.dispatch/handoff.md` in the main checkout (git-ignored, so it never lands
  in a PR), overwritten each wave. Post the same text as a comment on the wave's epic, so it survives
  a lost checkout. Keep it under a screen:

  ```markdown
  # Dispatch handoff, <date>, wave <k>
  - Merged: #a (PR #x), #b (PR #y)
  - Awaiting the owner (tier c, CI green): PR #z (#c), with what the Decision block asks
  - Open PRs still running or blocked: PR #w (#d): <state, worktree path>
  - Proposed follow-ups filed this wave: #e (impact) | added to epic #f: <polish items>
  - Next wave: #g, #h, ... (lanes below), and anything to hold back and why
  - Owner said (in their own words, this session): <quotes that still apply>
  ```

## Concurrency and lanes

- **At most 4 agents at once.**
- **One agent per shared library file.** Two agents never edit the same `lib/*.ps` (or the same
  `src/` module) at the same time. Plan each wave by file, not just by issue.
- **Generated artefacts are owned by one PR at a time:** `gallery/manifest.json` and everything
  `scripts/gallery.py sync` writes (the README highlights, `gallery/README.md`'s table,
  `gallery/show.sh`), index files, and committed renders. Two PRs that both regenerate them conflict
  every time; run them one after the other.
- **The model comes from the issue's `model:` label** (`model: haiku`, `model: sonnet`,
  `model: opus`). No label: the dispatcher picks one and adds the label.

## What every agent prompt says

The goals preamble at the top of the work-issue skill covers quality first, performance as a guard,
no Ghostscript timing and waiting for CI. The prompt adds only what the skill cannot know: the issue
number, the lane (which files are this agent's), and whether the PR is expected to need the owner.

## Merges

Agents merge their own tier (a) and tier (b) PRs (`SDLC.md`, "Who merges"). The dispatcher:

- **Batches tier (c).** Agents stop at green CI with a Decision block atop the PR. The dispatcher
  collects them and asks the owner once, with a list of PR links and what each Decision block asks,
  instead of interrupting per PR.
- **Never relays approvals into merges (tier d).** "The owner says merge it", passed on to an agent,
  is not a basis for an agent merge. The owner merges tier (c) PRs, or approves in the session where
  the merge runs.
- **Keeps the review log moving.** Tier (b) merges land in `docs/dev/review-log/pending/`; the owner
  reads it before the release (`docs/dev/review-log/README.md`).

## Follow-ups

Agents do not file issues. Each lists proposed follow-ups in its final report, with the
user-visible impact of each. The dispatcher:

- **files only those with a stated user-visible impact** (a wrong render, an error a user hits, a
  missing capability someone asked for);
- **adds polish ideas to the parent epic**, which is parked by default (the `deferred` label), not
  as new tickets;
- **never files anything that assigns physical work to the owner** (photographing references,
  buying hardware, printing samples).

## Filing an issue

`gh issue create --body` skips the issue forms in `.github/ISSUE_TEMPLATE/`, so a dispatcher-filed
issue carries the same two sections by hand. Agents read them at step 4:

```markdown
## Why / design intent
<what the change is for, and the design idea behind it>

## Perceptual acceptance check
<how a person would judge the result by eye, e.g. "reads easily at gallery size"; "n.a." for no visible output>
```

A metric in the issue (a budget, a count) sits next to the perceptual check, never instead of it,
and an agent must not optimise the metric at the check's expense (`docs/dev/TASTE.md`).
