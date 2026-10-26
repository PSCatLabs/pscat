# #772: SECURITY.md brought current for 0.7.0

SECURITY.md had gone stale in three steps: #740 (file writing), #763 (read
roots) and #735 (`writereals`/`readreals`). It still said `file` took only
`r` and that `writestring`, `writehexstring`, `filenameforall` and the rename
and delete operators did not exist. Its MCP section already covered the
pixel caps (#711), concurrent calls and cancellation (#622) and the render
jobs (#741), because #746 and #769 had updated it.

Every claim added was checked against `origin/main` at `6fea90e` and by
running a `release` build, not copied from the notes.

## What changed in the document

- "What counts as a vulnerability": writing outside `--allow-write` roots and
  escaping read roots are vulnerabilities; reading what the process can read
  (open by default) and writing inside a named root are not.
- New "Confining reads" and "Writes files" subsections: the roots, every path
  rule in `src/write_roots.rs` and `src/read_roots.rs`, `%stdout`/`%stderr`,
  `renamefile`/`deletefile`/`filenameforall`, `writereals`/`readreals`, and
  the accepted check-then-open race.
- MCP: ten tools (four job tools), a "never writes" bullet, a "read roots"
  bullet with the actual defaults (working directory, `$PSCAT_HOME`, the user
  data dir, `--allow-read`), and the Windows orphan caveat for
  `--exit-with-parent`. `--progress` and the job flags are described.
- Limits: a pixel row for the MCP cap, `--job-timeout` in the time row, and
  new "cannot be enforced" bullet for bytes written to a write root.

## Things found while checking

- `PSCAT_LIB` is only honoured under the MCP server when it lies inside a
  read root (it is skipped otherwise, as the embedded `lib/` answers), while
  `PSCAT_HOME` is itself a root. The old text said all three were trusted alike.
- A program can write `pscat: progress pages=N` to `%stderr` to move its own
  job's `pages_done`; the line is also stripped from the stderr returned. It is
  a status display only, so it is documented, not changed.
- The old claim that only the MCP server starts other programs was false:
  `pscat fonts install` runs `curl` (HTTPS only, SHA-256 chain checked). Now
  stated.
- `eval_postscript` does not return stderr on success, so a program's
  `%stderr` text is not shown there (the other tools return it as `Warnings:`).
- Embedded fonts (#757) change no security property beyond the licence, so the
  only edit is one sentence saying the standard 35 are found the same way.

## Not covered

- The `pstack`, `stack` and `==` operators still print unbounded (#745 is
  bounding that); only the `--pstack-on-error` dump is bounded (#703). The
  document does not describe either yet.
