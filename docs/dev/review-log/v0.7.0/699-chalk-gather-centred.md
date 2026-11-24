## chalk: centre gather rows like LaTeX (PR #722, issue #699, 2026-10-24)

- **What changed / what to look at:** a multi-row `gather`, `gather*` or `gathered` in `pscat chalk` now centres each row on its own width, as LaTeX does; before, the rows lined up on their first `=`, like `align`. `align` is unchanged (its before and after boards are byte-identical, so one image stands for both).
- **Perceptual check:** "A 3-row `gather` with rows of different widths renders with each row centred on the board, matching a LaTeX render's layout; `align` output is unchanged (byte-identical)."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/722#issuecomment-6381398052). No LaTeX install was available, so the reviewer judged against LaTeX's known `gather` layout, not a render.

| gather before | gather after | align (before = after) |
|---|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/892bbd74dd56ce3aca372b3b3910eafad4e31d06/docs/dev/review-log/pending/699-chalk-gather-centred/gather-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/892bbd74dd56ce3aca372b3b3910eafad4e31d06/docs/dev/review-log/pending/699-chalk-gather-centred/gather-after.png?raw=true) | ![align](https://github.com/PSCatLabs/pscat/blob/892bbd74dd56ce3aca372b3b3910eafad4e31d06/docs/dev/review-log/pending/699-chalk-gather-centred/align-before.png?raw=true) |
