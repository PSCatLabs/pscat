## MCP: chalkboard_math tool, LaTeX in and chalkboard PNG out (PR #710, issue #688, 2026-10-23)

- **What changed / what to look at:** a new MCP render path. `chalkboard_math` draws the `pscat chalk` board (#555) from LaTeX. Check that the tool's board is the CLI's board. The two are byte-identical.
- **Perceptual check:** the issue has none. Its intent: "This issue wraps that driver as an MCP tool and adds no new rendering."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/710#issuecomment-6380382211). polish: the thin superscript and the √ tail in the no-board overlay, the same on both paths.

| before (`pscat chalk`) | after (`chalkboard_math`) |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/fad7b2306081b96ea9f3b6e88a69421a44b9bbb0/docs/dev/review-log/pending/688-mcp-chalkboard-math/quadratic-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/fad7b2306081b96ea9f3b6e88a69421a44b9bbb0/docs/dev/review-log/pending/688-mcp-chalkboard-math/quadratic-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/fad7b2306081b96ea9f3b6e88a69421a44b9bbb0/docs/dev/review-log/pending/688-mcp-chalkboard-math/overlay-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/fad7b2306081b96ea9f3b6e88a69421a44b9bbb0/docs/dev/review-log/pending/688-mcp-chalkboard-math/overlay-after.png?raw=true) |
