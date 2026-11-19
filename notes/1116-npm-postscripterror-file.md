# 1116: npm `PostScriptError.file` and `runChain`

A `run` file's error report reads `Line: 5 (inner3.ps); Run chain:
inner3.ps:5 <- line 2` (#1043). `pscat-mcp` already split that into `line`,
`file` and `run_chain`; npm's `PostScriptError` read only the line number.

- `PostScriptError` now has `file` (the name in the `Line:` field's
  parentheses) and `runChain` (the chain as the interpreter wrote it, a
  string like `pscat-mcp`'s `run_chain`). Both are `undefined` for an error
  in the main program, so existing code is unaffected; `line` is unchanged
  (still the leading number).
- Parsing follows `ps_error_fields` in `src/mcp_server.rs`: split the
  `%%[ ... ]%%` row on `"; "`, then each part on the first `": "`. The file
  is whatever sits between the first ` (` and the closing `)`, so a name
  with spaces or parentheses survives. The interpreter escapes `;` and `]`
  in a run file's name as `%3B` and `%5D` (`report_name`, `src/interp.rs`), so
  a name cannot split the row; `file` and `runChain` carry the escaped
  spelling, as in `pscat-mcp`.
- Not parsed into an array: the chain is a display string, and the one place
  that structures it (`pscat-mcp`) keeps it a string too.
- Tests: `npm/test/api.test.mjs` feeds the constructor the report from
  `notes/1048-...`, an odd file name, a main-program report, and an end to end
  error inside a bundled `lib/` file.
