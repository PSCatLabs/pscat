## pscat sheet: labelled review sheets with mixed cell sizes (PR #696, issue #693, 2026-10-23)

- **What changed / what to look at:** a new output, `pscat sheet`: the #683 logo sheet (four candidates, each at 512 px, at 48 px true size and at 48 px enlarged 4x, labelled A-D) built by pscat alone. Before is the PIL-built sheet from #683; after is the one-command reproduction in the PR body.
- **Perceptual check:** "The #683 contact sheet (four logos, each at 512 and 48 px, labelled A–D) can be reproduced with a single pscat command, and looks at least as clear as the PIL version: labels legible, the 48 px renders shown at true size, nothing scaled or blurred."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/696#issuecomment-6377771155). Polish noted: the 48 px column leaves empty space; the PIL sheet's dark-mat 48 px variants are not reproduced (a follow-up, not required).

| before (PIL) | after (pscat sheet) |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/3ff4e5caed59168851d8ec756206d8e42c685ffd/docs/dev/review-log/pending/693-contact-sheet-labels/logo-sheet-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/3ff4e5caed59168851d8ec756206d8e42c685ffd/docs/dev/review-log/pending/693-contact-sheet-labels/logo-sheet-after.png?raw=true) |
