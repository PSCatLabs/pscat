# MCP `review_sheet` (#708)

`pscat-mcp` has a sixth tool: several PostScript programs in, one labelled
review sheet out. It is `pscat sheet` (#693) for an agent that has only MCP.
The same programs give the same bytes.

**Shape**
- Arguments: `entries` (rows, each `{source, label?, width?, height?, dpi?}`),
  top-level `width`, `height` and `dpi` as the defaults, `labels`, `sizes`,
  `out_path`. `dpi` is a list of cells, each a number or `{dpi, zoom}` (zoom
  2..16, the CLI's `9x4`). One shape only: the schema is a stability commitment.
- Programs are inline. The tool names no file, so it opens none and widens
  nothing; `out_path` is `.png` only and confined like every other tool's.
- A row with no `label` is labelled `program N` (the CLI's default is the file
  stem, which inline source does not have).
- The result is one image. A cell whose program fails is drawn as far as it got
  (as the CLI does) and the error text is returned beside the image, with the
  `limitexceeded` line when a limit stopped it.

**Why it does not shell out to `pscat sheet`.** The subcommand runs programs
in-process with no `--max-steps/--max-time/--max-memory` (those flags belong to
the flag CLI, and subcommands are dispatched before it), so a runaway program
would be bounded only by the server's kill timeout, with no memory bound. The
tool renders each cell in its own `pscat` child under `interpreter_args`, the
way `render_postscript` does, and composes the pictures in-process with the
library's `contact_sheet` pieces. Pages are opaque, so decoding a child's PNG
and blitting it gives the CLI's bytes (the acceptance test compares them, and
the #683 logo sheet, both ways).

**Caps** (all `output_too_large`, checked before any program runs): 32 cells,
the cells at most 40M pixels in all (before zoom), a cell at most 8000 px a side
even enlarged, the sheet at most 40M pixels and 8000 a side, labels at most 200
characters, all sources together at most `--max-input-bytes`. One time budget
for the call: each child gets what is left of `--timeout`, and under one second
left is a `timeout` error before the next spawn (never `--max-time 0`, which
means no limit). The sheet is composed in the server (4 bytes a pixel, 160 MB at
the cap), allocated before `--max-memory` counts.

**Drift.** The composition loop and the two type sizes are repeated from
`src/subcmd/sheet.rs`; a child's PNG whose size is not the planned one is an
`internal` error. The byte-identity test (labels, captions, zoom, mixed pages,
no-labels/no-sizes, defaults, a failing cell, a multi-page cell) is the guard.
Follow-up: `--max-*` flags on `pscat sheet` and the composer moved into
`pscat::contact_sheet`, so the tool shrinks to a call and cannot drift.
