## `pscat sheet`: sweeps, per-cell mat, stacked columns, `--cols` wrap (issue #706, 2026-10-24)

Follow-ups to #693 (`pscat sheet`), driven by the #683 logo review: a dark-backed 48 px variant
beside each candidate, a seed sweep with the seed under each cell, rows that wrap before 8000 px,
and a 48 px column that was mostly white.

**CLI.**
- `--sweep-seed SPEC` / `--sweep NAME=SPEC`, the next file only (like `--label`, so nothing sticks
  and no "clear" spelling is needed). The grammars, the identifier rule's shape and the value text are
  the flag CLI's: `parse_seed_spec`, `parse_sweep_spec` and `format_sweep_value` are private to
  `main.rs`, but a child module (`subcmd::sheet`) can call them, so there is one copy and
  `main.rs` is untouched. The `NAME` identifier check lives inline in `main.rs`'s `parse_args`
  (not in `parse_sweep_spec`), so it is repeated in `sheet.rs` with a test: the name is spliced
  into `/NAME v def`. A sweep takes every `--dpi` cell once per value (value by value). Its caption
  is the value (`seed=7`, `Density=20`), then `, WxH px` unless `--no-sizes`. One rule, whatever the
  number of dpi cells. The seed override is the one `--sweep-seed` uses; a seed sweep whose program never
  calls `srand` warns per file, as the flag CLI does.
- `--dpi` cell grammar is now `DPI[xZOOM][@MAT]`, columns separated by `,`, cells in a column by `+`.
  `MAT` is `#rgb`, `#rrggbb` (the `#` is optional: an unquoted `#` is a glob character in zsh),
  `white` or `black`.
- `--cols N`: wrap a row after N columns.

**Mat.** A matted cell is rendered on a transparent page (`set_transparent_background`, what
`--transparent` does), zoomed if asked, then drawn source-over onto a `MAT_PAD` (10 px) padded tile of
the mat colour, with the hairline outside the tile. So an opaque icon is framed by the mat (the PIL
sheet's look) and an icon with alpha shows the mat through. Nothing is scaled. The pad is a fixed
constant, not an option.

**Packing is opt-in.** Packing by default would change every existing sheet, which has to stay
byte-identical, so a column of several cells is spelled with `+`. A stacked column's cells
each carry their own caption directly under them (a single cell's caption still sits under the
row band, as before). The row is as tall as its tallest single image or stack; singles and stacks
are centred in it.

**Wrapping.** Columns are slots by index modulo N across every line and row, so lines line up; a slot is
as wide as its widest cell on any line. Lines of one row are `LINE_GAP` (10 px) apart, rows `ROW_GAP`
(18 px) as before; the header is above the first line only. If a row's cells vary in width
by position (a 512 px cell then 48 px ones) wrapping them mixes the sizes into shared slots: wrap
sweeps, whose cells are equal, rather than mixed-size rows.

**Library API (additions only).** `contact_sheet::{PackedRow, Layout::packed, composite_on_mat,
MAT_PAD, STACK_GAP, LINE_GAP}`. `Layout::new` now builds a `PackedRow` per row and calls
`packed(rows, None)`. What pins that: `single_column_layouts_match_the_documented_geometry` (positions
worked out by hand, for two rows with headers, mixed caption heights and an uncaptioned cell), the older
hard-coded layout tests, the exact-geometry tests in `tests/sheet.rs`, and a base-vs-head byte comparison of
`pscat sheet` and `--contact-sheet` runs (labels, no labels, zoom, mixed pages, a failing file,
accented/Korean/Thai labels, seed/param sweeps, `--grid`, `--transparent`), which was identical.

**Not done.** Sweeping several files at once (repeat the flag per file), two sweep axes at once, an
automatic packer (a greedy "fits under the tallest cell" pass would reorder cells the user wrote),
a mat note in the caption (it would widen the 48 px column this is meant to tighten).
