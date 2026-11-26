## README + site: showcase pscat chalk / chalkboard_math (PR #865, issue #849, 2026-10-26)

- **What changed / what to look at:** the front page's new "LaTeX on a chalkboard" section and the README's chalk image: four boards drawn at site-build time by `pscat chalk` (release build of main after #731 and #778), each with its exact command under it. New, so there is no before. The images below are 1000x500 downsamples; the overlay is shown composited on the site's dark ground (`#181c24`), as the page shows it.
- **Perceptual check:** the issue has no explicit one; its intent: "someone browsing crates.io, npm or the site" should discover `pscat chalk` from a rendered board with the exact command under it.
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/865). Polish noted: the ghost "F = ma" on the derivation board shares a row with f(x); the vector arrow over E sits tight against the E; the board's smudge blobs look slightly soft next to the chalk.

| board | after |
|---|---|
| quadratic (also the README image) | ![quadratic](https://github.com/PSCatLabs/pscat/blob/5e5fdfe6268e21522b58ec01d70f88f3abf6877f/docs/dev/review-log/pending/849-chalk-showcase/quadratic-after.png?raw=true) |
| aligned derivation, seed 7 | ![derivation](https://github.com/PSCatLabs/pscat/blob/5e5fdfe6268e21522b58ec01d70f88f3abf6877f/docs/dev/review-log/pending/849-chalk-showcase/derivation-after.png?raw=true) |
| pmatrix product, yellow | ![matrix](https://github.com/PSCatLabs/pscat/blob/5e5fdfe6268e21522b58ec01d70f88f3abf6877f/docs/dev/review-log/pending/849-chalk-showcase/matrix-after.png?raw=true) |
| `--no-board`, on a dark ground | ![overlay](https://github.com/PSCatLabs/pscat/blob/5e5fdfe6268e21522b58ec01d70f88f3abf6877f/docs/dev/review-log/pending/849-chalk-showcase/overlay-after.png?raw=true) |
