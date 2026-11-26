## imagemask paints the current pattern through its stencil in PNG, SVG and PDF (PR #1317, issue #1299, 2026-11-19)

- **What changed / what to look at:** an `imagemask` under `setpattern` shows the pattern instead of a flat colour; under the null pattern it paints nothing. Test page: a stencil under a stripe pattern (bottom left), an uncoloured dot pattern (bottom right), the null pattern (top left), solid blue (top right, unchanged). The specimen `examples/patterns.ps` panel 6 gains a heart stencil.
- **Perceptual check:** "the stencil shows the pattern, not a flat colour, in all three outputs."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1317#issuecomment-7010393197)

| output | before | after |
|---|---|---|
| PNG | ![before](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-png-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-png-after.png?raw=true) |
| SVG (Chrome) | ![before](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-svgchrome-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-svgchrome-after.png?raw=true) |
| PDF (gs) | ![before](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-pdfgs-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/im-pdfgs-after.png?raw=true) |
| specimen panel 6, PNG / SVG / PDF | ![png](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/panel6-png-after.png?raw=true) | ![svg](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/panel6-svgchrome-after.png?raw=true) ![pdf](https://github.com/PSCatLabs/pscat/blob/4091ae34111708c5e50b177d47b9502b51d4fe5d/docs/dev/review-log/pending/1299-imagemask-pattern/panel6-pdfgs-after.png?raw=true) |
