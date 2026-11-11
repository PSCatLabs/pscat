## chalk: row spacing inside a matrix or cases draws nothing (PR #951, issue #950, 2026-10-29)

- **What changed / what to look at:** `\\[4pt]` between the rows of a `pmatrix`/`cases` (in `pscat chalk` and `chalkboard_math`) no longer draws "[4pt]". The spacing is dropped, not honoured, so the board is the unspaced one byte for byte. `[x,y]` starting a matrix row stays content.
- **Perceptual check:** "A `pmatrix` with `\\[4pt]` between its rows shows no bracket text, and its rows sit sensibly spaced. A matrix whose row starts with `[x,y]` keeps that content. Existing chalk renders are byte-identical." (the issue's Perceptual acceptance check)
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/951#issuecomment-6512292134). polish: the spacing has no visible effect (consumed, as the issue allows).

| spaced before | spaced after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/spaced-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/spaced-after.png?raw=true) |

| with spacing (after) | without spacing (after) |
|---|---|
| ![spaced](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/spaced-after.png?raw=true) | ![unspaced](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/unspaced-after.png?raw=true) |

| `[x,y]` row before | `[x,y]` row after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/bracket-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/5fb32c1d99f0f546f23d8df0087ca330664a6116/docs/dev/review-log/pending/950-chalk-matrix-row-spacing/bracket-after.png?raw=true) |
