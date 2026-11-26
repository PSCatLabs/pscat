## pscat sheet: labels in the script they are written in (PR #715, issue #707, 2026-10-23)

- **What changed / what to look at:** `pscat sheet` row labels with accents, Korean, Japanese, Chinese or Thai now draw as those characters instead of `?`; a Latin-only label is unchanged to the byte. Look at the four labels (café, 한글, 日本語, ไทย). Known polish: the Latin label is bold while the Noto faces here are regular weight only.
- **Perceptual check:** "A sheet labelled `café`, `한글`, `日本語` and `ไทย` shows each label correctly."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/715#issuecomment-6380821878)

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/606ace4d7da757d67ba025ec0c05764a4ae93309/docs/dev/review-log/pending/707-sheet-non-latin-labels/labels-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/606ace4d7da757d67ba025ec0c05764a4ae93309/docs/dev/review-log/pending/707-sheet-non-latin-labels/labels-after.png?raw=true) |
