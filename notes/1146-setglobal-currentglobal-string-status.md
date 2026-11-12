## setglobal, currentglobal and `string status` (issues #1146 and #952, 2026-11-11)

Two PLRM gaps that failed whole programs, one PR.

**setglobal / currentglobal** (`src/ops/vm.rs`). A flag, `false` at start;
`typecheck` on a non-boolean, `stackunderflow` when empty. `save` pushes the
mode on a side stack keyed by save level (`VmMode`, one `Interp` field), so
`SaveRecord` and `do_save`/`do_restore` are untouched (held PRs #1070 and #1103
edit them). `restore` follows Ghostscript 10.07.1, not the PLRM's wording: the
mode after is global only if it is global now and was global at every save the
restore discarded (including restoring an outer save over an inner one saved
while local). `gsave`/`grestore`, `vmstatus` and a failed `setglobal` leave
it alone. Not added: `gcheck` and `globaldict` (the issue says "if they
exist").

**`string status`** (`src/ops/file.rs`). `pages bytes referenced created true`
or `false`, from `stat` only (the bytes are never read, so a FIFO costs
nothing). The name goes through `read_roots::open_confined` with a stat-only
reader, so the search order and the read roots are exactly `file`/`run`'s;
every refusal (outside the roots, `..`, a link out, NUL) is `false` like a
missing file, where `file` says `invalidfileaccess`, so the operator is no
oracle for files outside the roots (`pscat-mcp`). Measured against gs:
`pages` is `st_blocks` (the issue's "1024-byte blocks, rounded up" does not
match gs: 8 for 213, 1024, 1025 and 1500 byte files, 0 for an empty one),
`referenced` is mtime (not atime), `created` is ctime (not birth time).
Where we differ: a directory is `false` (gs `true`); the embedded `lib/`
copy is found as `run` finds it (times 0); a NUL cuts the name in gs and is
`false` here.
