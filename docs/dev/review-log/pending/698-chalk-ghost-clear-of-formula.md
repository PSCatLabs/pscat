## chalk: keep ghost writing out of the formula's box (PR #812, issue #698, 2026-10-25)

- **What changed / what to look at:** the faint ghost formulas on a `pscat chalk` board are now placed around the formula, never against it: 0.6 of the writing size clear above and below, 0.9 beside. A board the formula fills (the quadratic or a 2-row align on a small board) may have few or no ghosts; its eraser swipes and smudges remain.
- **Perceptual check:** "On a sweep of seeds (`pscat chalk` across 20 seeds on several formulas), no ghost fragment touches or crowds the formula: a reader never mistakes a ghost for a symbol. The board still looks used."
- **Reviewer:** PASS, round 3 (https://github.com/PSCatLabs/pscat/pull/812). Round 1 PASS on four pairs; round 2 FAIL on the 400 x 200 align (ghosts just past a row's end read as the row going on), fixed by wider margins; round 3 PASS on four 20-seed sheets and the pairs.

| integral before | integral after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/integral-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/integral-after.png?raw=true) |

| small align before | small align after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/align-small-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/align-small-after.png?raw=true) |

| quadratic before | quadratic after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/quadratic-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/quadratic-after.png?raw=true) |

The tightest case, the small 400 x 200 align, 20 seeds after the change:

![sweep](https://github.com/PSCatLabs/pscat/blob/ad6d388a76c8a01df5536d7bb330bd1b396638d6/docs/dev/review-log/pending/698-chalk-ghost-clear-of-formula/sweep-align-small-after.png?raw=true)
