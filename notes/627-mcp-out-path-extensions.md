## pscat-mcp: out_path takes render extensions only, no dot-directories (issue #627, 2026-10-16)

Follow-up to #568's `--out-root` confinement, tightened per least privilege
(it must land before the plugin ships, #569). `confine` in
`src/bin/pscat_mcp.rs` now takes the render's format and refuses:

- a name that does not end in `.png` / `.svg` / `.pdf` to match the format
  (case-insensitive; `handwrite` is `.png`; `.png` alone has no extension
  and is refused). A multi-page render is checked on the name asked for, so
  `pages.png` still gives `pages-001.png`.
- any directory below the root whose name starts with a dot, judged on the
  canonical path, so a symlink into `.git/` is refused and a root that itself
  sits under a dot-directory (`--out-root ~/.cache/renders`) still works.

Overwriting needs no separate check: the target is always `name` or
`stem-NNN.ext`, so an existing file can only be replaced by a render of its
own extension; `Makefile`, `.envrc` and `notes.txt` cannot be named at all.
`write_pages` now also refuses an existing target that is not a regular file
(it already refused symlinks and directories; this adds devices and FIFOs).
Both new refusals are `out_path_refused` and carry `required_extension`, so
a model can retry. Temp-file-then-rename and the symlink checks are
unchanged.

Not done: dot-file *names* that end in a render extension (`.x.png`) are
still allowed; the rule is about directories, as the issue asked.

Tests: `tests/mcp.rs` (extension per format, dot-directories incl. a symlink
to one, dot-directory root, overwrite of render vs non-render files), and a
`Makefile` case in `scripts/mcp_inspector_check.sh`.
