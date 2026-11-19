## render: composite fills and glyphs at 16 bits so faint layers stack to the float value (PR #1306, issue #731, 2026-11-19)

- **What changed / what to look at:** stacks of faint fills now reach the colour the arithmetic gives. The issue's 0.002 x 1000 square comes out at 221 instead of 128, and a splat cloud loses its grey core and its green and purple fringe. Everything else in the corpus moves by 4 levels at most.
- **Perceptual check:** the issue has no written check. Its intent: "Stacking many faint translucent fills never reaches the colour the maths says it should, and stops early ... colours also drift in hue."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1306#issuecomment-7003657298)

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/1d3887d09d4bf2cd7f30d111f529a2fe2a866d17/docs/dev/review-log/pending/731-16bit-compositing-shadow/cloud-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1d3887d09d4bf2cd7f30d111f529a2fe2a866d17/docs/dev/review-log/pending/731-16bit-compositing-shadow/cloud-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/1d3887d09d4bf2cd7f30d111f529a2fe2a866d17/docs/dev/review-log/pending/731-16bit-compositing-shadow/issue-stack-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1d3887d09d4bf2cd7f30d111f529a2fe2a866d17/docs/dev/review-log/pending/731-16bit-compositing-shadow/issue-stack-after.png?raw=true) |
