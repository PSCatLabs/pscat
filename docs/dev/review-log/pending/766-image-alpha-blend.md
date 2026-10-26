## image: setalpha and setblendmode apply to image, imagemask and colorimage (PR #830, issue #766, 2026-10-26)

- **What changed / what to look at:** images and stencils now take `setalpha` and `setblendmode` (Normal, Multiply) in PNG, SVG and PDF. Each sheet is one page of `selftest/drivers/image_alpha.ps`, four rows: before (PNG), after (PNG), after (SVG in Chrome), after (PDF via Ghostscript). The three after rows should agree; the cells meant to be opaque (page 3, after grestore and initgraphics) should look like the before row.
- **Perceptual check:** the issue has none; judged against its stated intent: "Apply them to images too, in the PNG, SVG and PDF outputs".
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/830, "Perceptual review, round 1")

| page | before / after (PNG), after (SVG), after (PDF) |
|---|---|
| 1: Normal at 0.5 | ![p1](https://github.com/PSCatLabs/pscat/blob/0242d56366ccae695975c0f25854e9f92e26ac60/docs/dev/review-log/pending/766-image-alpha-blend/image_alpha-p1-before-after.png?raw=true) |
| 2: Multiply | ![p2](https://github.com/PSCatLabs/pscat/blob/0242d56366ccae695975c0f25854e9f92e26ac60/docs/dev/review-log/pending/766-image-alpha-blend/image_alpha-p2-before-after.png?raw=true) |
| 3: clip and state resets | ![p3](https://github.com/PSCatLabs/pscat/blob/0242d56366ccae695975c0f25854e9f92e26ac60/docs/dev/review-log/pending/766-image-alpha-blend/image_alpha-p3-before-after.png?raw=true) |

Reviewer polish note: the PDF row's clipped disc edge (page 3, cell 1) is slightly more anti-aliased than the PNG and SVG rows (Ghostscript's own rasteriser; not visible at normal size).
