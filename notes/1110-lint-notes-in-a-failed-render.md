## A failed `render_postscript` keeps the lint note that explains it (issue #1110, 2026-11-18)

`/ln (x) def` followed by a call to `ln` fails with an error that never says `ln`. The CLI's
`[shadowed-operator]` lint (#1052) names the cause, but `--lint` printed nothing after an error
(#947) and `split_lint_findings` dropped every finding after `[run-error]` (#942), so an MCP render
of that program returned the error alone.

- `lint::ERROR_EXPLAINING_CHECKS` / `explains_an_error` (new, one list) name the checks that still
  hold after an abort. Only `shadowed-operator`: it reads `userdict`, which keeps every `def` that ran
  before the error. `blank-page`, `gsave-imbalance`, `dict-leak`, `stack-leak` read where the program
  ended, so they stay dropped (the #942 misleading blank-page finding does not come back).
- CLI `print_lint`: after the `[run-error]` line it prints the kept findings (and the band `note:`
  only when one is kept). Still returns true and prints no `--lint-strict` footer for the error.
- MCP `split_lint_findings` keeps the same lines, so the result has the `PostScript error:` block and
  then a separate `Lint:` block, the same shape as a successful render's. Plain and banded, `render_postscript`
  and `start_render`.
- A `%lint: allow shadowed-operator` comment still silences it.

An old `pscat-mcp` build that clears everything on `[run-error]` just keeps hiding it; the output
change is additive.
