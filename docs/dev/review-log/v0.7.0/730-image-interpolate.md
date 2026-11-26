## image: honour /Interpolate true (PR #771, issue #730, 2026-10-25)

- **What changed / what to look at:** images whose dict sets `/Interpolate true` are now smooth when magnified (PNG bilinear, PDF key, SVG `image-rendering`). Check that the keyed render is smooth and close to Ghostscript's (third column), and that nothing changes without the key.
- **Perceptual check:** the issue has none. Its intent: an image with `/Interpolate true` scales smoothly as in Ghostscript.
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/771). polish: the right cloud's glow sits very slightly lower in ours than in gs; worst channel difference from gs is 6 of 255.

| before (hard blocks) | after | gs 10.07 |
|---|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/repro-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/repro-after.png?raw=true) | ![gs](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/repro-gs.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/cloud-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/cloud-after.png?raw=true) | ![gs](https://github.com/PSCatLabs/pscat/blob/bf2931c3dbc86a51fb47304ff1f963f6ac218d9f/docs/dev/review-log/pending/730-image-interpolate/cloud-gs.png?raw=true) |
