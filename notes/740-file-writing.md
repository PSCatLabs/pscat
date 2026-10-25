## File writing: `--allow-write`, `renamefile`, `deletefile`, `filenameforall` (issue #740, 2026-10-25)

Owner decision on the issue: writing is an interpreter capability; whether an agent may write is
policy for the agent-facing layer. So the interpreter gained write roots, empty by default, and
`pscat-mcp` never sets any.

**What shipped.**
- `Interp::set_write_roots(&[dirs])` and the CLI's repeatable `--allow-write DIR`. They're applied
  wherever the CLI builds an interpreter: headless, window, `-i`, REPL and sweep frames.
  `--spool` refuses the flag. The self-test modes refuse it like any other flag.
- `(name) (w) file` (truncate or create), `(a) file` (append or create). `r+`, `w+` and `a+` are
  `invalidfileaccess`. `(%stdout)` and `(%stderr)` need no root. Other `%device` names are
  `undefinedfilename`.
- New operators: `write`, `writestring`, `writehexstring` (lowercase hex, as Ghostscript
  writes), `flush`, `renamefile`, `deletefile`, `filenameforall`. `flushfile` and `closefile`
  handle output files. Reading an output file, or writing an input or closed one, is `ioerror`.
- `Interp::writable_path(name)` is the helper later writers (#733 `writeobject`, #735
  `writereals`) should reuse. Better still, they can take a file object from `(w) file` and
  write through `write_bytes` in `src/ops/file.rs`.

**The path check** (`src/write_roots.rs`, the only place that decides):
- Any `..` component is refused before any lookup, even one that would stay inside.
- A relative name resolves against the current directory only, never the read search order
  (`$PSCAT_HOME`, the user data dir, the exe dir).
- An existing target is canonicalised in full (following symlinks). A new file's parent is
  canonicalised. The result must be strictly inside a canonical root; `Path::starts_with`
  compares whole components, so `/out` doesn't contain `/outside`.
- A dangling symlink is refused. Opening it would create its target, wherever that is.
- On Unix, an existing regular file with `nlink > 1` is refused for writing, because another of
  its names may be outside. This goes beyond the issue's spec, and applies to opening only.
- A new file is opened with `create_new` (O_EXCL), which won't follow a final-component symlink
  even if one appeared after the check. If it loses that race to a regular file, the file is
  re-checked as an existing one.
- `renamefile` and `deletefile` act on the directory entry. Only the parent is canonicalised and
  the last component is taken as given, so deleting an in-root symlink removes the link, never
  its target. Directories are refused as delete targets and as rename destinations.
- Names that aren't UTF-8 are `invalidfileaccess`, so the check never runs on a lossily
  converted name.

**Threat model.** The program is the adversary. No operator creates symlinks or hard links, so
any link it could abuse already sat in a root. The check and the open aren't atomic: a *different*
local process with write access to the root could swap a directory for a symlink in between. That
attacker can already write there, so this is accepted rather than reaching for `O_NOFOLLOW` or
`openat`, which would need `libc` as a dependency (`Cargo.toml` is an owner path).

**Disk writes are unbuffered.** The CLI's single-run path exits without dropping the interpreter
(the dict-teardown skip in `main.rs`), so a `BufWriter` would lose a tail the program never
closed. Each `writestring` is one `write(2)`. For the issue's cache use, build big strings and
write them in a few calls.

**`filenameforall`.** Reads are unrestricted (any path the user can read), so listing follows the
same policy:
- An absolute template lists its directory.
- A relative template lists the directory in every place `run` looks (`paths::matching_names`),
  plus the embedded `lib/` names, so `(lib/*.ps)` works in an install with no `lib/` on disk.
- Results are sorted and deduplicated, regular files only.
- Wildcards (`*`, `?`, `\` quoting) match within the last component. A wildcard in a directory
  part matches nothing, which is simpler than Ghostscript's recursive walk.
- It runs as a `forall` over `{ scratch copy proc exec }`, built from the operator objects, so
  redefining `copy` or `exec` doesn't affect it and `exit` works as usual.

**Not done.** `writeobject` (#733), `writereals` (#735), `fileposition`/`setfileposition`,
`resetfile`, `%stdin`, encode filters, and a scratch-dir flag for `pscat-mcp`, which the owner
said could come later. wasm/npm have no disk: `%stdout` writes land in the captured output, and
every disk path is refused.
