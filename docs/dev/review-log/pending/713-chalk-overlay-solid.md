## chalk: the no-board overlay's radical bar and small strokes stay solid (PR #821, issue #713, 2026-10-26)

- **What changed / what to look at:** drychalk's new `/Firm` (default 0, byte-identical) holds a long stroke's weight to its end and presses short strokes harder; `pscat chalk --no-board` and the MCP `board: false` turn it on. Look at the √ (bar and diagonal) and the superscript. Boards are byte-identical.
- **Perceptual check:** "On the quadratic formula with `board:false` at seed 1 and a few other seeds, the superscript and the radical tail read as solid chalk strokes at the same apparent weight as the main symbols. Boards with a background look no worse."
- **Reviewer:** PASS, round 2 (https://github.com/PSCatLabs/pscat/pull/821#issuecomment-6410948399; round 1 was before the Codex lift fix). polish: the 2 is still slightly lighter than b, a, c, and the √ tail a touch thinner than the main strokes.

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/51851f53fe013714e6ab16ae134221518b90daa7/docs/dev/review-log/pending/713-chalk-overlay-solid/quadratic-overlay-s1-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/7b120022d6c5bbedcd06b4911c503cc83f805491/docs/dev/review-log/pending/713-chalk-overlay-solid/quadratic-overlay-s1-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/51851f53fe013714e6ab16ae134221518b90daa7/docs/dev/review-log/pending/713-chalk-overlay-solid/quadratic-overlay-s2-radical-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/7b120022d6c5bbedcd06b4911c503cc83f805491/docs/dev/review-log/pending/713-chalk-overlay-solid/quadratic-overlay-s2-radical-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/51851f53fe013714e6ab16ae134221518b90daa7/docs/dev/review-log/pending/713-chalk-overlay-solid/integral-overlay-yellow-s1-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/7b120022d6c5bbedcd06b4911c503cc83f805491/docs/dev/review-log/pending/713-chalk-overlay-solid/integral-overlay-yellow-s1-after.png?raw=true) |
