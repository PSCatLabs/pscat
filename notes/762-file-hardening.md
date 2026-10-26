## File hardening after `--allow-write` (issue #762, 2026-10-25)

Follow-ups from #740. All five items shipped; Windows recursion did not.

**Byte cap.** `Interp::set_max_write_bytes(Option<u64>)` and the CLI's `--max-write-bytes N`
(bytes, or `k`/`m`/`g` binary suffix; `0` is no cap). Counted when a program hands bytes to
`write`, `writestring`, `writehexstring` or `writereals` (not `%stdout`/`%stderr`), for the
interpreter's whole life. Over the cap is an ordinary `limitcheck`: nothing is written, and
`writereals` checks its whole size before it opens (truncates) the target. Default: the CLI applies
1 GiB when `--allow-write` is given (same figure as `UNTRUSTED_MAX_MEMORY`); the library applies
none. Decisions:
- It is not a field of `Limits`: that struct's `is_none` guards the interpreter's fast path, and
  the sibling flags end the run uncatchably. A catchable `limitcheck` is safe here because a
  program that catches it can only fail to write more.
- The count is per interpreter, not reset by `begin_source` (the REPL calls it per line, which
  would make the cap per line).

**Descriptor cap.** `MAX_OPEN_OUTPUT_FILES` = 64 live disk output files; one more is `limitcheck`,
checked before the open so nothing is truncated. The interpreter holds `Weak` handles
(`Interp::open_outputs`), pruned at each open, so an unreferenced file frees its slot.

**Buffering.** `Sink::Disk(DiskSink)`: 64 KiB buffer, own implementation (not `BufWriter`) so
`resetfile` can discard it and `fileposition` can include it. A failed write sets a sticky error
(next write / `closefile` is `ioerror`) because `showpage`/`quit` flush and cannot raise. Flushed
by `closefile`, `flushfile`, `flush`, `showpage`, `quit`, any by-name read (`read_resource`), and
`DiskSink::drop`. `Interp::flush_outputs()` flushes every registered file. The CLI calls it on
every exit path (`run_headless` incl. error runs, REPL EOF and `quit`, the two window loops) and
exits non-zero if it fails; `Drop for Interp` also calls it, before the `keep_dicts_on_drop`
return, as a backstop. `tests/file_hardening.rs` keeps the file in userdict (which the CLI never
frees) so only a flush can write it. I checked that the explicit flush alone keeps that test green, and that with it and the
backstop both removed the test fails.

**Operators.** `fileposition`, `setfileposition`, `resetfile`.
- Input `(r)` files (in-memory bytes; `currentfile` too): position in `0..=length`; the line count
  is recomputed so error reports stay right.
- Disk output: flush, then `stream_position` / `seek`; a position past the current length is
  `rangecheck`, so there are no sparse holes (which would dodge the byte cap). An `(a)` file
  seeks but still writes at its end (`O_APPEND`).
- `%stdout`, `%stderr`, filters and closed files: `ioerror`. Negative or past-end: `rangecheck`.
- `resetfile`: output discards its buffer and the sticky error; input is a no-op (no buffer
  here); closed is a no-op (Ghostscript: "a no-op, not an error").

**`filenameforall` on Windows.** Decision: `\` stays the escape character on every platform
(PLRM; one template means one thing everywhere). `/` already separated directories, drive
letters and `//server/share` included, so Windows paths work as `C:/dir/*.ps`; documented in the
`src/filenames.rs` module doc and SECURITY.md. A `#[cfg(windows)]` unit test and a cross-platform
integration test use a `/` path. Rewriting `\` to `/` was rejected: `C:\dir\*.ps` would turn
`\*` into a wildcard-escape. Recursive patterns: not done (a wildcard in a directory part still
matches nothing).

**Not done / follow-ups.** A cap on the number of files created in sequence; a per-program
(rather than per-interpreter) byte count if an embedder runs many programs on one `Interp`
(they can call `set_max_write_bytes` again); `pscat-mcp` and the subcommands still never write.
