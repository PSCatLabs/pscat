## pscat sheet: readable captions for a sweep (issue #1000, 2026-11-18)

`pscat sheet --sweep Panel=1,2,3` drew `Panel=1`, `Panel=2`, `Panel=3` under the
cells, which read as unfinished on the public hybrid comparison sheets.

**What changed.** Two options on `pscat sheet` (`src/subcmd/sheet.rs`):

- `--captions LIST` gives each sweep value a caption of your own, in place of
  `seed=7` or `Density=20`. It applies to the next file's sweep only (like
  `--label`), takes one comma-separated entry per value (a count mismatch, no
  sweep, a repeat, or no file after it are refused), and an empty entry leaves
  that cell's caption out. A caption cannot contain a comma: the grammar is the
  flag CLI's comma lists. The issue's example called the flag `--labels`; that
  would sit beside `--label` (a row label) and `--no-labels`, so it is
  `--captions`.
- `--no-values` omits the sweep value from the captions. The pixel size stays
  unless `--no-sizes` also says so, and when both are off the caption strip
  goes, so the sheet is shorter. It parallels `--no-sizes`.

A stderr failure line (`... at 72 dpi, Panel=2 failed:`) still names the value,
not the caption, so a message stays greppable. Sheets made without the new
options are unchanged to the byte.

**The hybrid sheets.** The panels already title themselves ((a) The image model
alone ...), so the five sheets under `docs/guides/hybrid/` are re-rendered with
`--no-values` and the recipes in `examples/hybrid/README.md` and the headers of
the `compare_*.ps` files say so. The panels are pixel-identical to the committed
sheets (checked by diffing the rows above the old caption strip); the sheets are
21 px shorter, which is the caption strip.

**MCP.** `review_sheet` has no sweep input (a sweep is a flag-CLI/`pscat sheet`
feature; MCP callers send one entry per value and label it), so there was no
sweep caption to mirror and the schema is unchanged.
