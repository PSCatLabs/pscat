## chalk: raw LaTeX to a chalkboard, pscat::chalk + pscat chalk (PR #694, issue #555, 2026-10-23)

- **What changed / what to look at:** a new output, `pscat chalk 'LaTeX' -o board.png`: the formula fitted and centred on a dressed slate (or green) board, faint erased ghost writing along the edges; `--no-board` gives chalk alone on a transparent page. Also the guide image `docs/dev/design/drymedia/chalk_cli_demo.png`. New, so there is no before.
- **Perceptual check:** the issue has no explicit one; its intent: "Deterministic LaTeX → chalkboard image ... the equation written in chalk on a board", and "`irregular: 0` produces a clean, untilted layout".
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/694#issuecomment-6373419376). Polish noted: the leaf-shaped "0" (strokeglyphs, legible) and a ghost fragment that can sit near the formula.

| board | after |
|---|---|
| quadratic, default | ![quadratic](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/quadratic-after.png?raw=true) |
| integral with limits, yellow | ![integral](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/integral-after.png?raw=true) |
| pmatrix | ![pmatrix](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/pmatrix-after.png?raw=true) |
| cases, green board | ![cases](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/cases-after.png?raw=true) |
| two-row align, irregular 0 | ![align](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/align-clean-after.png?raw=true) |
| --no-board, on a dark ground | ![no board](https://github.com/PSCatLabs/pscat/blob/48d608c0ceb2390ffaafcc4db897a711392647c8/docs/dev/review-log/pending/555-chalk-latex-cli/noboard-on-dark-after.png?raw=true) |
| guide image | ![demo](https://github.com/PSCatLabs/pscat/blob/26276b35600253e9742d1c716d7daf65f65f7105/docs/dev/design/drymedia/chalk_cli_demo.png?raw=true) |
