## chalk: a bracket after a row break is content unless it is a length (issue #936, 2026-10-29)

`pscat chalk 'a \\ [x,y] = 0'` drew the second row as `= 0`: `chalk::split_rows`
took any `[...]` after a top-level `\\` (or `\\*`) as TeX's row-spacing option
and dropped it. A commutator at the start of a row is common in quantum
mechanics, and the content vanished with no error.

- `split_rows` now drops the bracket only when `is_tex_length` reads its inside
  as a length: an optional sign, a number (`.` or `,` decimal) and one of TeX's
  units (`pt pc in bp cm mm dd cc sp em ex`, any case, `true` allowed), or an
  optional factor and a length register, recognised by TeX's naming (a name
  ending in skip, sep, width, height, depth, indent, amount, length, rule or
  limit, plus `\jot` and `\fill`: `\baselineskip`, `0.1\textheight`, `\arraycolsep`). A closed
  list first; Codex (round 1) showed it turned real spacing such as
  `\\[\abovedisplayskip]` into an error; round 2 added the length, rule
  and limit endings (`\unitlength`, `\fboxrule`). A register outside the
  naming (a package's own) is content, and mathkit names it. With optional `plus` and `minus` parts (TeX glue:
  `2pt plus 1pt minus .5pt`, `0pt plus 1fil`). Everything else, `[x,y]`, `[1]` and `[\pi]` among
  them, stays as the next row's first characters.
- No error. LaTeX stops with "Illegal unit of measure" on `\\ [x,y]`; drawing
  the intended math is friendlier, and the standard spacing options still work.
- Whitespace (newlines too) between `\\` and the bracket is skipped, as TeX
  skips it.
- Not changed: inside `pmatrix`/`cases`/`\substack`, mathkit splits the rows
  itself and never drops a bracket, so `[x,y]` was already safe there; a real
  `\\[2pt]` there draws `[2pt]` as text (a follow-up if anyone hits it).
- Docs: `pscat chalk --help`, `docs/guides/dry-media.md`, psart's
  `board-math.md` mention the rule and `\left[ ... \right]`.
- Tests: unit cases in `src/chalk.rs`; `tests/chalk.rs`
  `a_row_starting_with_a_bracket_keeps_it` measures the second row's width
  (fails with the old split). No pinned board moves.
