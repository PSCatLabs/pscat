# PR template: the self-contained Decision block (#1164, follow-up to #1147)

`.github/pull_request_template.md` carried the old three-line Decision block
(Look at / Choice / Recommendation). #1147 made the work-issue skill write a
self-contained one instead, so a PR opened by hand started from a different,
thinner format.

The template's commented block now holds the skill's fill-in block verbatim
(source issue and parent, why the PR exists, why the merge is the owner's, the
three decision lines, numbered "What to do" steps, a before/after table with
SHA-pinned image URLs). A short comment header says when to use it and points at
`.claude/skills/work-issue/references/perceptual-review.md`, "The Decision
block", for the full rules, so the format has one source of truth. The block is
still commented out: it is copied to the top of a PR body only when the owner
must act.

No code or test reads the template; `scripts/pr_body_lint.py` passes on it.
