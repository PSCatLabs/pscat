## MCP: review_sheet tool for labelled review sheets (PR #721, issue #708, 2026-10-24)

- **What changed / what to look at:** a new MCP render path. `review_sheet` draws the `pscat sheet` review sheet (#693) from inline programs. Check that the tool's sheet is the CLI's sheet: the two are byte-identical (the #683 logo sheet here: four candidates, each at 512 px, 48 px and 48 px enlarged 4x).
- **Perceptual check:** "An MCP client can produce the #683 logo sheet in one tool call, and it matches the CLI output byte for byte."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/721#issuecomment-6381361561)

| before (`pscat sheet`) | after (`review_sheet`) |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/5d5310b1be4416021fedd039c8397ff815ba872c/docs/dev/review-log/pending/708-mcp-review-sheet/logo-sheet-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/5d5310b1be4416021fedd039c8397ff815ba872c/docs/dev/review-log/pending/708-mcp-review-sheet/logo-sheet-after.png?raw=true) |
