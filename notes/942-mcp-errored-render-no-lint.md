## MCP: an errored render carries no Lint block (issue #942, 2026-10-29)

`render_postscript` and `start_render` (plain and banded) ran the child with
`--lint`. For a run that ended in a PostScript error the CLI prints
`[run-error]` and then the end-of-run checks, and the server dropped only the
`[run-error]` line (#369). So `1 0 div` came back with the error and a
`Lint:` block saying `blank-page`, which reads as a second, separate problem.

`split_lint_findings` in `src/mcp_server.rs` now drops every finding when the
child reported `[run-error]`. All the lint checks look at the end state
(a page with no ink, a `gsave`/`begin` still open, values on the stack, a page
count short of `%%Pages:`), and an aborted run ends wherever the error hit:
`(unterminated` leaves no ink, an error inside `gsave 5 dict begin` leaves both
open. The `PostScript error:` block carries the real news and the partly drawn
page is returned as before.

`eval_postscript` and `handwrite` do not run `--lint`, so they were not
affected; `tests/mcp.rs` pins that too.

Not done: the CLI's own `--lint` still prints those consequence findings under
`[run-error]` (`src/main.rs`, outside this change). Dropping them there too
would make the two agree.
