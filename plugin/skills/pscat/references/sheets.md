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

A sweep caption defaults to the value (`Density=20`). To name the cells instead,
give `--captions` one entry per value (the next file's sweep only; commas
separate the entries, so a caption cannot hold one; an empty entry leaves that
cell's caption out). To hide the value, because the cells already say what they
are, use `--no-values` (the pixel size stays unless `--no-sizes`):

```sh
pscat sheet -o d.png --page 300x300 --sweep Panel=1,2,3 \
    --captions '(a) image model,(b) pscat,(c) hybrid' piece.ps
pscat sheet -o d.png --page 300x300 --no-sizes --no-values --sweep Panel=1,2,3 piece.ps
```

## Over MCP: `review_sheet`

With only the MCP server connected, call `review_sheet` instead: the programs
go inline as `entries: [{source, label, width, height, dpi}]` (never file
names), `dpi` is a list of cells, each a number or `{"dpi": 9, "zoom": 4}`, and
`width`, `height` and `dpi` at the top level are the defaults for entries that
give none. The sheet comes back as one PNG, the same bytes the CLI writes;
`labels: false` and `sizes: false` drop the text. A failing program is drawn as
far as it got, with its error beside the image. At most 32 cells and 40 million
pixels; each cell runs under the server's interpreter limits.

The sweeps, mats and wrapping of `pscat sheet` are inputs too (a server older
than this lists no `sweep`, `mat`, `cols` or `values` in the tool's schema):

- `sweep` on an entry draws that program once per value, every `dpi` item for
  each value: `{"seed": [1, 2, 3]}` (each seed also seeds noise, as above) or
  `{"param": "Density", "values": [10, 20, 40]}` (the program starts with
  `/Density 20 def` made, so it reads it with `/Density where { pop Density }
  { 10 } ifelse`). Only lists of numbers, not the CLI's `1:12` ranges. Each cell
  is captioned `seed=7` or `Density=20`.
- `captions` on an entry gives each sweep value words of its own, in order and
  one per value (`["(a) image model", "(b) pscat"]`; an empty string leaves
  that cell's caption out). `values: false` at the top level leaves the value
  out of the captions, for cells that already say what they are.
- `mat` in a `dpi` item (`{"dpi": 9, "mat": "#222"}`; `#rgb`, `#rrggbb`,
  `white` or `black`) draws the page transparent and sets it on that colour,
  padded 10 px.
- `cols` at the top level wraps a row of more than that many cells onto further
  lines, the label only above the first.

Twelve seeds, four to a line, is one call:
`{"entries": [{"source": "<piece.ps>", "width": 640, "height": 640, "dpi": [36],
"sweep": {"seed": [1,2,3,4,5,6,7,8,9,10,11,12]}}], "cols": 4}`. Stacked columns
(`+`) are not available over MCP yet.
