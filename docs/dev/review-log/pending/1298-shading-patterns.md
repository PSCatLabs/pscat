## Shading patterns: makepattern PatternType 2 in PNG, SVG and PDF (PR #1367, issue #1298, 2026-11-20)

- **What changed / what to look at:** a new feature and its new specimen sheet (`examples/shading_patterns.ps`). A shading is the current colour, filling text, strokes, circles and an imagemask. Shown here in all three outputs: PNG, the SVG rasterised by Chrome, and the PDF rasterised by gs. The base build stops at `makepattern` with a `rangecheck`, so the before image is a blank page.
- **Perceptual check:** "gradient-filled text matches across the three outputs."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1367#issuecomment-7035290266). Polish noted: gs's raster of the PDF has aliased glyph edges, because gs paints pattern fills without anti-aliasing.

| before | after, PNG | after, SVG (Chrome) | after, PDF (gs) |
|---|---|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/b1a05f93b6a03488c33ec675a498bb180826f158/docs/dev/review-log/pending/1298-shading-patterns/specimen-before.png?raw=true) | ![png](https://github.com/PSCatLabs/pscat/blob/b1a05f93b6a03488c33ec675a498bb180826f158/docs/dev/review-log/pending/1298-shading-patterns/specimen-after-png.png?raw=true) | ![svg](https://github.com/PSCatLabs/pscat/blob/b1a05f93b6a03488c33ec675a498bb180826f158/docs/dev/review-log/pending/1298-shading-patterns/specimen-after-svg-via-chrome.png?raw=true) | ![pdf](https://github.com/PSCatLabs/pscat/blob/b1a05f93b6a03488c33ec675a498bb180826f158/docs/dev/review-log/pending/1298-shading-patterns/specimen-after-pdf-via-gs.png?raw=true) |
