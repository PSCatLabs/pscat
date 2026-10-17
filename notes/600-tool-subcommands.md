## handwrite, typewrite and photo-etch are `pscat` subcommands (issue #600, 2026-10-16)

Part of release epic #562 (split from #564, after #598's resolver and
embedded `lib/`). The MCP server and the published skills should need only
`pscat` on PATH: no bash scripts, no checkout. This also makes `pscat-mcp`'s
`handwrite` tool work on Windows (it used to shell out to bash).

**What was built.**
- `src/subcmd/{mod,handwrite,typewrite,photo_etch}.rs`, dispatched by a
  three-line check at the top of `main` (`subcmd::dispatch`). Each port builds
  the PostScript driver the script built (the library's bytes, then the
  options dict) and renders it through the same `run_headless` /
  `finish_headless` as `pscat --png driver.ps`, so halftone, the trailing-art
  page, `-001` numbering, SVG and PDF output are the CLI's own code, not a
  copy. The library comes from `paths::open("lib/...")`: the compiled-in copy
  unless a checkout, `$PSCAT_HOME` or the cwd says otherwise.
- The measuring pre-passes (`hs-linecount`, `tw-pageheight`, `et-dims`) run in
  their own fresh `Interp` on a default page with stdout captured, as the
  scripts' separate `pscat --headless` did, so the real render does not
  inherit their random state.
- `scripts/handwrite.sh`, `typewrite.sh` and `photo_etch.sh` are now
  thin wrappers (find a binary, `exec pscat <sub> "$@"`; `$PSCAT_BIN`
  overrides for all three). They no longer export `PSCAT_HOME`.
- `pscat-mcp` runs `pscat handwrite ... -- <text>`; `repo_root`, the
  `PSCAT_ROOT` override and the three-levels-up fallback are deleted, and the
  sibling binary is found with `EXE_SUFFIX` (`pscat.exe` on Windows).
- `scripts/package_release.sh` no longer bundles `scripts/`, and its
  self-check now runs the three subcommands from the bundle.
  `scripts/clean_install_test.sh` does the same from a `cargo install`ed
  binary and asserts every `lib/` file came from the embedded copy.
- `tests/subcommands.rs` (cross-platform: no bash except one `cfg(unix)`
  wrapper smoke test) and `tests/typewrite.rs` (now drives the subcommand).

**Byte-identity with the old scripts.** Verified by hand against the
`origin/main` scripts (copied to a scratch tree with a symlink to the new
binary as `pscat`, so both ran the same renderer): 9 handwrite cases, 8
renders plus one matching failure (`--seed 1985.0`) (defaults,
seeds, ruled, ink, `--halftone --dpi 144`, explicit height, newlines and blank
lines, capitals and non-ASCII, CRLF and bare CR, `--`), 14 typewrite cases (memo fields with
parens and backslashes, `--height fit`, a 5-page document with page numbers,
tabs/indent/pitch/margins, `--reflow`, trailing blank lines, a worn
machine at 100 dpi, memo-only, stdin, CRLF, PDF, and the `--ps` driver itself)
and 10 photo-etch cases (the fixtures in `tests/data` plus
`examples/etching_source.jpg`, EXIF-rotated, custom ink/paper/angle/dpi, SVG,
PDF, a path with parens and a backslash): every PNG, SVG and PDF is
byte-identical, as are the exit codes. Failures match too (`--seed 1985.0`
dies in `srand` with the same error in both). PDFs carry no timestamp, so
they compare byte for byte. The permanent regression test compares each
subcommand with a driver `tests/subcommands.rs` assembles itself.

**Differences, on purpose.**
- **Options are validated.** The scripts spliced `--ink`, `--size`,
  `--seed`... into PostScript unchecked (SECURITY.md). Now a numeric option
  must be a plain decimal number, `--ink`/`--paper` exactly three of them,
  `--paper` `plain` or `ruled`. The user's own token is spliced, not a
  reparsed float (`36` and `36.0` are an integer and a real in PostScript: the
  library's `srand` rejects a real seed). Text is escaped as before
  (`\`, `(`, `)`). SECURITY.md is updated.
- `--width`/`--height`/`--max-dim` must be whole numbers 1..8000 up front (the
  scripts failed later, in `--page`).
- `handwrite` lowercases ASCII only (`tr` lowercased by locale, so an accented
  capital became its lowercase twin; both are invisible in this font, so the
  rendered page is the same). A CR or CRLF in the text is turned into a newline before escaping; pscat's
  lexer reads a raw CR (or CRLF) in a string literal as a newline too, so the
  string is the same (checked: `(a\rb)` and `(a\r\nb)` both give length 3 and
  a 10 in the middle).
- A text that starts with `-` goes after `--`; `--help` prints the usage to
  stdout and exits 0, and an unknown option fails with the usage on stderr,
  as before.
- `pscat --help` now lists the subcommands.
- `typewrite` on text with bytes that are not UTF-8: the script's second
  `sed` died with "illegal byte sequence" and typed a truncated page; the
  port types the page (a stray byte stays a stray byte, as in the first sed).
- A file literally named `handwrite`, `typewrite` or `photo-etch` given as
  `pscat`'s first argument is now a subcommand; `./handwrite` still works.

**Not done.** Windows is covered by the same code paths and tests (no bash
in `tests/subcommands.rs` but the one `cfg(unix)` test), but this PR could
only run macOS locally; CI on Windows arrives with #601. The published
skills still show the `scripts/*.sh` forms (they keep working through the
wrappers); #568 rewrites them.
