## functional-postscript docs: agent logs removed, duplicate PNG dropped, design notes merged (#1169)

`docs/functional-postscript/` is the design record behind `lib/functional.ps` and
`lib/aspect.ps`. It had come to main with working files that were not documentation.

Removed (git history and PR #923 keep them): `PROGRESS.md` (an agent work log),
`REVIEW-HANDOFF.md` (the review brief, already marked historical),
`BROWSER-VALIDATION.json` and `LIVE-BROWSER-VALIDATION.json` (validation
transcripts). Nothing in them needed rescuing: the measured numbers are in
`MEASUREMENTS.json`, and the caveats REVIEW-HANDOFF listed are already in API.md,
AOP.md, IMPLEMENTATION.md and TESTING.md.

Merged: `DESIGN.md` (the Phase A plan) into `DESIGN-DECISIONS.md`, which now holds scope,
the execution-model problem, research sources, the numbered decisions and future
directions. The plan's validation section (duplicated by TESTING.md) and its
workflow-exception section (the experiment's branch policy) were dropped.

`ENGINE-FRICTION.md` stays, with its experiment-era framing replaced. A1 (`usertime`
is wall time) was re-checked against `src/clock.rs` and still holds.

`assets/imperative.png` was byte-identical to `assets/ultraviolet.png` (the point of
the control). Only `ultraviolet.png` is kept; `index.html` shows it for both figures,
`functional_demo.py` and `check_functional_docs.py` no longer produce or expect the
second file, and `poster_variations_match_the_imperative_control` still compares the
two drivers' full pixel buffers.

`scripts/check_functional_browser.mjs` now prints its report instead of writing
`LIVE-BROWSER-VALIDATION.json` into the docs tree. The index page's stale
"clone this branch" instructions now clone the default branch.
