## ops: nulldevice and ImageType 3 masked images (PR #758, issue #739, 2026-10-25)

- **What changed / what to look at:** `ImageType 3` masked images now render (a 2x2 colour image under an 8x8 stencil, plus a rotated copy; before, the program stopped at `typecheck` and drew nothing), and marks inside `gsave nulldevice ... grestore` no longer appear (before, `nulldevice` was `undefined` and the orange square and text after it were lost). The Ghostscript renders match.
- **Perceptual check:** none in the issue; judged against its intent ("a picture with a stencil mask" paints through the mask; `nulldevice` discards marking) and TASTE.md.
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/758#issuecomment-6404436996)

| before | after |
|---|---|
| ![masked before](https://github.com/PSCatLabs/pscat/blob/19bbe9f0822eabd727f566d18037b0ef7fc63b0c/docs/dev/review-log/pending/739-nulldevice-imagetype3/masked-before.png?raw=true) | ![masked after](https://github.com/PSCatLabs/pscat/blob/19bbe9f0822eabd727f566d18037b0ef7fc63b0c/docs/dev/review-log/pending/739-nulldevice-imagetype3/masked-after.png?raw=true) |
| ![nulldevice before](https://github.com/PSCatLabs/pscat/blob/19bbe9f0822eabd727f566d18037b0ef7fc63b0c/docs/dev/review-log/pending/739-nulldevice-imagetype3/nulldev-before.png?raw=true) | ![nulldevice after](https://github.com/PSCatLabs/pscat/blob/19bbe9f0822eabd727f566d18037b0ef7fc63b0c/docs/dev/review-log/pending/739-nulldevice-imagetype3/nulldev-after.png?raw=true) |
