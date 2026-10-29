# Sweeps, contact sheets and review sheets

Read this to explore a design space (many seeds or parameter values in one
run) or to compare several files or sizes side by side with labels. Back to
[SKILL.md](../SKILL.md).

## Sweep a seed or parameter (explore a design space in one run)

```sh
# Reseeds every `srand` call: works on found art unmodified.
pscat file.ps --sweep-seed 1:12 --contact-sheet grid.png

# Predefines /NAME to each value in turn; the source opts in with
# `/NAME where { pop NAME } { default } ifelse`.
pscat file.ps --sweep Density=10,20,40,80 --contact-sheet grid.png
```

One sweep axis per run, `A:B` / `A:B:STEP` / `A,B,C`, at most 64 frames.
Needs `--png` (numbered frames) and/or `--contact-sheet PATH` (one grid
PNG; `--grid COLSxROWS` sets the layout). A frame that errors still
writes its partial canvas; the exit code is nonzero if any frame failed.

## Build a labelled review sheet (several files, several sizes)

`--contact-sheet` is for one program's same-size sweep frames. To compare
different files, or one file at different sizes (logo candidates at 512 px
and 48 px, say), with names under them, use `pscat sheet` instead of reaching
for PIL:

```sh
pscat sheet -o sheet.png --page 384x384 --dpi 96,9,9x4 \
    --label "A: one-line cat" a.ps --label "B: drawing board" b.ps
```

Each file is a row; each entry in `--dpi` is a cell in it, drawn at its own
true pixel size and centred in its column, with the row label above and a
`512x512 px` caption below. `9x4` adds the 9 dpi render again enlarged 4x with
no smoothing, to read a small icon pixel by pixel. `--page` and `--dpi` apply
to the files after them, so one sheet can mix page sizes. `--no-labels` and
`--no-sizes` drop the text; the sheet is at most 8000 px a side. An erroring
file is still drawn, as far as it got, and the exit code is nonzero.

Sweeps, mats, stacks and wrapping (all in the one command):

```sh
# logo candidates: 96 dpi, then a column of the 9 dpi render and its dark-mat copy
pscat sheet -o logos.png --page 384x384 --dpi '96,9+9@#222' a.ps b.ps
# twelve seeds of one piece, labelled seed=1 .. seed=12, four to a line
pscat sheet -o seeds.png --page 640x640 --dpi 36 --cols 4 --sweep-seed 1:12 piece.ps
# a parameter sweep, labelled Density=10 and so on
pscat sheet -o d.png --page 300x300 --dpi 72 --sweep Density=10,20,40,80 piece.ps
```

`--sweep-seed` and `--sweep NAME=` take the next file only and use the same
grammar as the flag CLI. A seed sweep also seeds the native noise
operators (`N noiseseed`), so noise art varies per cell; a program that calls
`noiseseed` itself keeps its own table. A `--dpi` cell is `DPI[xZOOM][@MAT]`: `@#222` (or
`@white`) draws the page transparent, sets it on that colour and pads it 10 px,
to judge an icon on dark and light. `+` joins cells into one stacked column.
`--cols N` wraps a row after N columns. Quote a `--dpi` list with `#` or `+`.

## Over MCP: `review_sheet`

With only the MCP server connected, call `review_sheet` instead: the programs
go inline as `entries: [{source, label, width, height, dpi}]` (never file
names), `dpi` is a list of cells, each a number or `{"dpi": 9, "zoom": 4}`, and
`width`, `height` and `dpi` at the top level are the defaults for entries that
give none. The sheet comes back as one PNG, the same bytes the CLI writes;
`labels: false` and `sizes: false` drop the text. A failing program is drawn as
far as it got, with its error beside the image. At most 32 cells and 40 million
pixels; each cell runs under the server's interpreter limits.
