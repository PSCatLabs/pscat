<!-- cloud: replace-file -->
# Review sheets: several programs, sizes or seeds side by side

Read this to compare candidates (logo options, a seed's variants, one drawing at
several sizes) in one labelled picture. Back to [SKILL.md](../SKILL.md).

Call `review_sheet`. Each entry is a row, and each item of its `dpi` list is a
cell in that row, drawn at its own true pixel size and centred in its column,
with the row label above and a pixel-size caption below (`512x512 px`):

- `entries`: the rows, each `{source, label, width, height, dpi}`. `source` is
  the PostScript program, inline (there are no file names here). `label`
  defaults to `program N`. `width` and `height` are the page in points.
- `dpi`: a list of cells, each a number or `{"dpi": 9, "zoom": 4}`. The zoom
  draws that render enlarged 4 times with no smoothing (whole factors 2 to 16),
  to read a small icon pixel by pixel. `dpi`, `width` and `height` at the top
  level are the defaults for entries that give none.
- `labels` and `sizes`: `false` drops the row labels or the size captions.

A program that shows several pages gives its last one. A program that fails is
still drawn as far as it got, and the reply lists the error for that cell.
There are at most 32 cells, and the cells and the sheet have pixel and time
limits that `get_renderer_info` reports.

**To compare seeds or parameters**, write one entry per value and put the value
at the top of each program: `7 srand 7 noiseseed` for a seed (`srand` seeds
`rand`; the native noise operators `gnoise2` and `fbm2` keep their own table, so a
noise piece needs `noiseseed` too), or `/Density 20 def` for a parameter that the
program reads with `/Density where { pop Density } { 10 } ifelse`. Give each
entry a label such as `seed=7`, and use a small `dpi` (36 or so) so a dozen
cells fit on one sheet. If a program already sets its own seed (an `N srand`
line), change that line in each entry instead of adding another.

**To judge an icon at small sizes**, give one entry a `dpi` list such as
`[96, 9, {"dpi": 9, "zoom": 4}]`.
