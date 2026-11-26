## pscat sheet: seed/parameter sweeps, per-cell mat, stacked columns, --cols wrap (PR #723, issue #706, 2026-10-24)

- **What changed / what to look at:** `pscat sheet` can sweep seeds or a parameter with each cell captioned by its value, set a cell on a dark or light mat, stack small cells in one column (`+`) and wrap rows (`--cols`). Look at the logo sheet's dark-mat 48 px cells and the 12-seed sheet's labels.
- **Perceptual check:** "The #683 logo sheet with a dark-mat 48 px variant per row, and a 12-seed sweep with seed labels, each come from one `pscat sheet` command. A reviewer can read every label, and nothing is scaled."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/723#issuecomment-6381427133). Polish: the mat cell's caption does not say it is on a mat.

Logo sheet, before (`--dpi 96,9`) and after (`--dpi '96,9+9@#222'`):

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/logo-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/logo-after.png?raw=true) |

12 seeds of gallery/woven_labyrinth.ps, before (unlabelled `--contact-sheet`) and after (`--sweep-seed 1:12 --cols 6`):

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/seeds-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/seeds-after.png?raw=true) |

Stacked column, `'96,9,9@#222'` against `'96,9+9@#222'`:

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/pack-flat.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/e1de5003df59fb6eed6d34828578657a485da611/docs/dev/review-log/pending/706-sheet-sweeps-mat-cols/pack-stacked.png?raw=true) |
