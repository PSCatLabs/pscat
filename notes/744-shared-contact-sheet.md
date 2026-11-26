# One composition path for `pscat sheet` and `review_sheet` (#744)

`src/subcmd/sheet.rs` and the `review_sheet` MCP tool each had their own
composition loop and their own copy of the 16/12 px label and caption sizes,
kept in step only by `tests/mcp.rs`' byte-identity test. Both now call
`pscat::contact_sheet::{SheetPlan, compose_sheet}` (`src/contact_sheet.rs`):

- `SheetPlan::new(&[SheetRow], &SheetOptions)` sets every label and caption
  (value, then `WxH px (shown at Zx)`), checks each cell's size after zoom and
  mat (`cell_image_size`) and lays the sheet out (`Layout::packed`). No cell has
  rendered yet, so a caller can still refuse the sheet by its `layout()`.
- `compose_sheet(&plan, |row, cell| frame)` allocates the sheet and, for each
  cell in order, asks the closure for the frame, then zooms, mats, frames and
  captions it. One frame is in memory at a time.

What stays in each caller: how a cell is rendered (the CLI in process with the
per-cell limits and seed or parameter sweep; the tool in a child under the one
`--timeout`), the caller's own caps (`MAX_CELLS`; the tool's `--max-pixels`) and
its error wording. The tool passes no mat, no stacked columns and no wrap.

A pure refactor: every `pscat sheet` PNG, stderr line and exit code is unchanged
(checked base against head), and the old `contact_sheet` items are untouched.
One ordering difference on input that fails anyway: all cell sizes are probed
and capped before the first label is set, where the CLI used to set each row's
text as it went.

Not done here (scope added to #744 by later comments, for follow-up): expose
mats, stacked columns, `--cols`, sweeps and captions in `review_sheet`; give
`pscat sheet` one `--timeout` budget shared across cells; caption a matted cell
("on #222").
