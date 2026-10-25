# MCP: stderr warnings on success (follow-up from #747, #728)

`pscat` exits 0 and prints a one-line stderr note when a font from an
uninstalled pack falls back to a built-in face (#728). The MCP tools kept a
child's stderr only when the run failed, so an MCP user never saw it.

`warnings_content` (src/mcp_server.rs) turns a successful child's stderr into
one `Warnings:` text content item: blank lines, `pscat: wrote ...` (the
child's receipt, naming a scratch path) and `pscat: lint: ...` lines are
dropped, each distinct line shows once in first-seen order, and the item is
bounded (8 lines, 300 characters each, then `... and N more`). Used by
`render_postscript`, `handwrite`, `chalkboard_math` and, over its successful
cells, `review_sheet`; a failed run still reports its whole stderr in the
error. `eval_postscript` is unchanged (its stderr is already the report).

The test uses a generic line, a caught error reported with `handleerror`
(exit 0, `%%[ Error: ... ]%%` on stderr), so it does not depend on #747.
