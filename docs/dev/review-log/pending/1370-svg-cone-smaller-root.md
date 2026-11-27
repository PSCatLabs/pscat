## SVG: a radial whose circles are not nested paints its smaller root (PR #PRNUM, issue #1370, 2026-11-27)

- **What changed / what to look at:** SVG (not PNG or PDF, which were right) showed a hole where a cone-shaped radial shading, with its end not extended, should paint the smaller circle's disk. The SVG now has two gradients and matches the PNG. Images are the SVG rasterised by resvg; the third column is pscat's PNG of the same program.
- **Perceptual check:** none written on the issue; judged against its intent: the SVG paints the smaller root where the PNG, PDF and gs do.
- **Reviewer:** PASS, round 1 (PR #PRNUM comment "Perceptual review, round 1")

| before (SVG) | after (SVG) | PNG reference |
|---|---|---|
| ![shfill before](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_shfill-before.png?raw=true) | ![shfill after](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_shfill-after.png?raw=true) | ![shfill reference](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_shfill-png-reference.png?raw=true) |
| ![pattern before](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_pattern-before.png?raw=true) | ![pattern after](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_pattern-after.png?raw=true) | ![pattern reference](https://github.com/PSCatLabs/pscat/blob/bac06a5704ac2ddbfe5f2b720062735131d3e764/docs/dev/review-log/pending/1370-svg-cone-smaller-root/cone_pattern-png-reference.png?raw=true) |
