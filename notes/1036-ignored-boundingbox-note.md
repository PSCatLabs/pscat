## A note when a non-EPSF `%%BoundingBox` is ignored (issue #1036, 2026-11-11)

Only an EPSF file's `%%BoundingBox` sizes the page (`--bbox` extends that to
any file), but nothing said so at run time: a `%!PS-Adobe-3.0` piece with a
720x486 box rendered on a 612x792 page with the art in the bottom 486 pt, and
the half-blank PNG looked like a bug in the art. `Options::resolve_page` now
prints, on stderr:

    pscat: note: %%BoundingBox 720x486 ignored (not EPSF); pass --bbox or --page

- **When.** The file is not EPSF, `--bbox` and `--page` were not given, and its
  box (the one `--bbox` would use, `bbox::ignored_box`, hidden) is usable and
  a different size from the page in use (letter by default, `--bbox`'s
  rounding of the box otherwise). A missing or malformed box in a plain file
  says nothing: the file never asked for it. One line per run, also under
  `--bands` and `--sweep-seed` (they resolve the page once).
- **Stdout is untouched** (the `pscat: wrote` receipt is the only thing on it).
- **Compared with the default page, not with a `setpagedevice` the program
  may make later.** A program that sets `/PageSize` equal to its box still
  gets the note; it is a hint, and the program's page wins either way.
- **MCP.** No server change: `render_postscript` already shows a successful
  child's stderr under `Warnings:` (`notes/mcp-stderr-warnings.md`), so the
  note arrives there as is. The hint names `--bbox`/`--page`, which the tool
  spells `bbox` and `width`/`height`; the words are close enough to follow.
- Tests: `tests/bbox_ignored_note.rs` (CLI), `tests/mcp.rs`
  (`a_plain_files_ignored_bounding_box_is_a_warning`).
