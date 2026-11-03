## chalk: row spacing inside a matrix or cases; a stale skill example (issue #950, 2026-10-29)

`pscat chalk 'A = \begin{pmatrix} a & b \\[4pt] c & d \end{pmatrix}'` drew
`[4pt]` in front of the second row. #936 taught the top-level row split to
drop a `\\[<length>]`, but the `\\` inside `pmatrix`, `bmatrix`, `vmatrix`,
`cases` and `\substack{}` is left for mathkit, which splits those rows
itself and drew the bracket as text.

- `chalk::layout` now runs each row through `drop_row_break_options`, which
  pairs backslashes left to right as `split_rows` does and, after every `\\`
  left in the row, drops a `*` and a bracket that `is_tex_glue` reads as a
  length. The bracket test is `row_break_options_end`, which `split_rows`
  now calls too: one classifier for both levels.
- **Consumed, not honoured**, as at the top level. Rendered side by side, a
  `pmatrix` of fractions and a `cases` with `\\[4pt]`/`\\[2pt]` dropped look
  the same as the unspaced input and are not cramped: mathkit already sets
  each row gap from the rows' heights and depths, which is what a TeX author
  usually adds `\\[4pt]` for. Honouring it would mean a new way to pass a
  length into mathkit and a PostScript-side reading of it.
- A bracket that holds no length stays the row's content (`\\ [x,y] & c`),
  as before. A nested `\\*` now drops its `*` as the top level does; before,
  mathkit drew it. No tracked file used one (`git grep -F '\\*'`). A
  `\\[2pt]` inside `\text{...}` loses its bracket the same way; unlikely.
- Only `pscat chalk` and the `chalkboard_math` MCP tool get this (both go
  through `chalk::driver`). Calling mathkit's `mkpath` from PostScript, a
  `[2pt]` after `\\\\` is still drawn; psart's `board-math.md` says so.
- Docs: `pscat chalk --help`, `docs/guides/dry-media.md`, psart's
  `board-math.md`. pscat's `handwriting-chalk.md` quoted `\oplus` as its
  unsupported-command example, which works since #948; it now quotes
  `\aleph`. No other skill, guide, `--help` or MCP text named a command
  that works now as unsupported.
- Tests: unit cases in `src/chalk.rs`
  (`row_spacing_inside_matrices_goes_too`); `tests/chalk.rs`
  `row_spacing_inside_a_matrix_draws_nothing` checks the spaced and unspaced
  boards are the same PostScript and PNG bytes (fails with the pass removed),
  and that `[x,y]` in a `bmatrix` keeps its width.
