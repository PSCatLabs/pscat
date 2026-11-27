## drykit: a /From cut at a touch band change joins two pieces without a ring (PR #1478, issue #1444, 2026-11-27)

- **What changed / what to look at:** a stroke drawn as two pieces (`/To c` then `/From c`) where its touch changes band at the cut, against the same stroke drawn whole (top of each image). Before, there was a darker or lighter arc at the join; after, the join matches the whole stroke. Only partial (`/From`/`/To`) drawing changes; whole strokes and `/From 0 /To t` are byte-identical.
- **Perceptual check:** "draw a `/Width 8` dkstroke whole and as two incremental pieces cut at 0.5, at several touch bands; the join must not show a ring."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1478#issuecomment-7225211465)

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/rising-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/rising-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/falling-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/falling-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/returning-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/returning-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/tilt-chalk-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/1ee10bf81799db710137bb993748f64f0bb7a611/docs/dev/review-log/pending/1444-drykit-band-change-cut/tilt-chalk-after.png?raw=true) |
