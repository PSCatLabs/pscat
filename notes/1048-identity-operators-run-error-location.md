## Identity operators; run-file error locations (issues #1048, #1043, 2026-11-06)

**#1048.** `product` → `(pscat)`, `version` → the crate version string
(`env!("CARGO_PKG_VERSION")`, so `(0.7.0-rc.4)` today), `revision` →
`major*10000 + minor*100 + patch`, `serialnumber` → 0, in
`src/ops/level2.rs` beside `languagelevel`. Scheme for `revision`: two
decimal digits each for minor and patch (good while both stay under 100),
the pre-release tag dropped, so 0.7.0-rc.4 and 0.7.0 are both 700. That
reads "0.7.0 or later" as `revision 700 ge`, which is what a library gating
on a feature wants; telling an rc from its final release is not something a
program should need. gs answers these too (`(GPL Ghostscript)`, 10071), so
the guide's example checks `product` first. The capabilities catalog lists
pscat extensions only (CAPABILITIES.md), so these PLRM operators are not
cataloged; they are documented in `docs/guides/getting-started.md`
("Which pscat is running").

**#1043.** A `run` file now gets a named scanner (`Lexer::run_file`, the
name as the program gave it). `Lexer::line` reports its lines as it does
the main program's; `Interp::last_file` follows `last_line` (a pointer
compare per token, no refcount traffic in the common case). The report:

    %%[ Error: undefinedresult; OffendingCommand: div; Line: 5 (inner3.ps); Run chain: inner3.ps:5 <- line 2 ]%%

- **Compatibility.** A main-program error's report is byte-identical (the
  `; Line: N ]%%` exact-string tests across the suite are untouched).
  `Line:` still starts with the number, so npm's `/Line:\s*(\d+)/` reads 5.
  `pscat-mcp`'s `ps_error_fields` splits `5 (inner3.ps)` into `line` 5 and
  `file`, and gives the chain as `run_chain`.
- **The chain** is the estack's line-bearing scanners, innermost first,
  snapshotted in `recover` on the uncaught path only (a `stopped` used as
  control flow pays nothing), before any caller could unwind the stack. It
  is shown only while its innermost entry still matches the reported
  location, and cleared by `begin_source` and by a top-level `stop` (which
  has already drained the stack). Capped at 8 named frames, then `...` and
  the outermost. The main program shows as `line N`: the interpreter is
  given bytes, not a file name, and teaching every CLI entry point to pass
  one was out of this lane.
- **Escaping.** `;` and `]` in a file name are `%3B`/`%5D`, so a name cannot
  forge the `; ` field separator or the ` ]%%` end that `pscat-mcp` splits on.
- **Not done: procedure bodies.** An error inside a procedure reports the
  line of the token being scanned when it ran (the call site), as before.
  Reporting the line a procedure's token was *scanned* on would need a
  source position per array element (a parallel position table in
  `PsArray`, or a field on `Object`): the object model change ROADMAP's
  escalation rule exists for, not a cheap addition. A procedure defined
  and called inside the same run file does report that file's call line.
- **Still unnamed:** `(f.ps) (r) file cvx exec` (a file object `exec`ed
  directly has no name to report), eexec streams and executable strings
  keep the sticky outer location, as before.
