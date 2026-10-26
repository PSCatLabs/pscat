# #763: pscat-mcp confines file reads to read roots

Owner decision (2026-10-25, on the issue): restrict the folders. The
interpreter has the capability; the MCP layer restricts what agents can do.

## What shipped

- `src/read_roots.rs`: `ReadRoots` (canonical directories, built with the
  same `canonical_dirs` helper as `src/write_roots.rs`) and `open_confined`,
  which wraps the resource resolver's reader. `Interp::set_read_roots`,
  `clear_read_roots`, `read_roots()`; `None` (open) by default.
- Enforced in `ops::file::read_resource` (both the plain and the
  execution-limited branch), which `(r) file`, `run` and `readreals` use, and
  in `filenames::matching_names` (`filenameforall`). `lib/etching.ps`'s photo
  path goes through `(r) file`, so it is covered too.
- CLI: `--read-root DIR` (repeatable) and `--confine-reads` (confined, no
  directories). Refused with `--spool`. `pscat handwrite` takes both.
- `pscat-mcp`: roots = working directory (not `/`, mirroring `out_path`),
  `$PSCAT_HOME` and the per-user data dir when they exist, plus repeatable
  `--allow-read DIR` (a bad one stops the server at startup). Every child
  gets `--confine-reads` first, so an empty list still confines.

## Design calls

- **Skip, don't fail, per candidate.** A relative name tries several places;
  a place outside the roots is skipped like a missing file, so
  `(lib/artkit.ps) run` falls through to the embedded copy when the
  exe-dir copy is outside the roots. Only if nothing answers and something
  was refused is the result `invalidfileaccess`.
- **Missing outside is `invalidfileaccess`** for an absolute name, judged by
  the deepest existing ancestor, so the error never says whether a file
  outside exists. A missing file inside stays `undefinedfilename`.
- **`..` anywhere is refused** before lookup, as for writes, even when it
  would stay inside.
- **The canonical path is what is read**, narrowing the check/read window to
  the write side's threat model (a concurrent local writer swapping links).
- **No regular-file or hard-link rule for reads.** Inside a root, a FIFO is
  the user's own (the #702 FIFO test in `tests/mcp.rs` still gets
  `limit_exceeded`). `/dev/zero` is outside every root unless someone names
  `/dev`; a symlink to it inside a root resolves outside and is refused.
- **`filenameforall` outside yields nothing, no error.** The PLRM lists no
  `invalidfileaccess` for `filenameforall`, and Ghostscript's `-dSAFER`
  skips files it may not open the same way. Directories outside are not
  even listed; a symlink inside pointing out is not yielded.
- **Fonts are not gated.** `findfont` matches the requested name against
  file stems in the font directories and never joins a program string into
  a path; user code cannot load a font file by path.
- **No MCP tool takes an input path.** `review_sheet` takes source strings,
  so the #708/#744 question is settled by the roots alone.

## Follow-ups

- The hosted server (#689) should build its `Interp` with `set_read_roots`
  (probably an empty list: only the embedded `lib/`).
- SECURITY.md is #772's.
- New readers (#733's file ops/scanner work) must go through
  `read_resource`/`open_confined`.
