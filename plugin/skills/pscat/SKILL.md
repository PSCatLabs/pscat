---
name: pscat
description: Render, preview, and debug PostScript with the pscat interpreter — PNG/SVG/PDF out, lint, seed sweeps, handwriting-style notes, typewritten letters and memos, photo-to-engraving, watch-it-draw window, halftone mode. Use for any .ps/.eps file work, generating diagrams or art in PostScript, producing notes, cards and lettering in a natural, hand-drawn script, or typed documents, or when the pscat MCP tools are available.
---

# pscat — PostScript as a tool

`pscat` is a PostScript (Level 2) interpreter: one binary, with its
PostScript library (`lib/*.ps`) compiled in, so `(lib/artkit.ps) run`
works from any directory. Check it is installed with `pscat --help`; if
it is not, install it: a release from
<https://github.com/PSCatLabs/pscat/releases>, or
`cargo install --git https://github.com/PSCatLabs/pscat`
(installs `pscat` and `pscat-mcp`).

If the **pscat MCP tools** are connected (`render_postscript`,
`eval_postscript`, `handwrite`, `chalkboard_math`, `describe_art_capabilities`), they do the
same jobs with images returned inline; see "MCP" below.

## Render a file (the common case)

```sh
pscat --png out.png file.ps            # headless render to PNG
pscat --svg out.svg file.ps            # vector SVG (one file per page)
pscat --pdf out.pdf file.ps            # multi-page PDF
pscat --page 500x500 --png out.png f.ps  # canvas in points (default 612x792)
pscat --dpi 300 --png out.png f.ps     # print resolution (default 72)
pscat --halftone --png out.png f.ps    # screen like a mono laser printer
pscat --transparent --png out.png f.ps # no page background
generate | pscat --png out.png -       # `-` reads the program from stdin
```

Multi-page documents number their pages: `out.png` → `out-001.png`,
`out-002.png`, … SVG does the same; PDF is naturally multi-page.

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

With only the MCP server connected, call `review_sheet` instead: the programs
go inline as `entries: [{source, label, width, height, dpi}]` (never file
names), `dpi` is a list of cells, each a number or `{"dpi": 9, "zoom": 4}`, and
`width`, `height` and `dpi` at the top level are the defaults for entries that
give none. The sheet comes back as one PNG, the same bytes the CLI writes;
`labels: false` and `sizes: false` drop the text. A failing program is drawn as
far as it got, with its error beside the image. At most 32 cells and 40 million
pixels; each cell runs under the server's interpreter limits.

## Debug a program

```sh
pscat --headless file.ps               # run without rendering; errors → stderr
pscat --headless --pstack-on-error f.ps  # + gs-style operand stack post-mortem
pscat -e '3 4 add ='                   # evaluate a snippet
pscat --lint --png out.png file.ps     # self-check the render (see below)
```

Exit code 0 = clean run; nonzero = a PostScript error, reported on
stderr as `%%[ Error: undefined; OffendingCommand: bogus; Line: 3 ]%%`
(the line is given for errors in the top-level program, not inside a
`run`-loaded library). `--png` still writes the partial canvas after an
error: useful to see how far a program got.

**`--lint`** catches mistakes that don't *error*: a blank page, an
unbalanced `gsave`/`grestore`, stuff left on the operand/dict stack.
Findings print to stderr as `pscat: lint: [check] message` (or
`pscat: lint: clean`); the exit code is unchanged (`--lint-strict` makes
findings fail). A run that errored prints `pscat: lint: [run-error]`.

## Handwriting-style notes (string → PNG)

pscat's handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's. If a request asks for a named or real person's
hand, or for a document that passes as written by someone, decline that part
and offer a generic hand instead (the default /HandScript, another `--seed`).

```sh
pscat handwrite "meet me at the bandshell at nine" -o note.png
pscat handwrite --paper ruled --size 30 --seed 7 "text"
pscat handwrite --ink 0.4,0.1,0.1 --jitter 24 "shakier, redder"
```

The /HandScript dynamic font, a natural, hand-drawn script: jittered strokes, no two letters alike,
reproducible per `--seed`. Word-wraps and auto-sizes the height.
**Lowercase only**: input is lowercased, and characters outside a–z 0–9
`.,-'!?` advance invisibly. `pscat handwrite --help` lists every option.
From PostScript: `(lib/handscript.ps) run` (one options dict; see its
header).

## LaTeX on a chalkboard (formula → PNG/SVG/PDF)

```sh
pscat chalk 'x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}' -o board.png
pscat chalk 'f(x) &= (x+1)^2 \\ &= x^2 + 2x + 1' --seed 7 --color yellow -o derivation.png
pscat chalk --file eq.tex --json '{"board": {"tone": "greenboard"}}' -o eq.png
pscat chalk 'e^{i\pi} + 1 = 0' --no-board -o overlay.png    # transparent, chalk only
```

Raw LaTeX with **single** backslashes (unlike mathkit from PostScript),
`\\` between rows, `&` to align; `$...$` or an `align` wrapper is fine.
Deterministic per `--seed`. The block is shrunk to fit the board and centred.
TeX outside mathkit's subset fails with one line naming it
(`unsupported LaTeX command \oplus`) and writes nothing. `--config FILE` /
`--json TEXT` take the JSON config (`layout`, `chalk`, `board` or
`"board": false`, `seed`); `pscat chalk --help` and the dry-media guide list
every key. `--print-ps` prints the generated PostScript.

With the MCP tools connected, use **`chalkboard_math`** for a chalkboard or
an equation on a board: `{"latex": "x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}", "seed": 7}`
(JSON escaping only; the LaTeX itself has single backslashes) returns the
PNG inline. The same keys as flat arguments (`size`, `color`, `board` or
`false`, `ghosting`, `width`/`height`, `dpi`, ...), `config` for the rest; a
TeX error is a one-line `latex_error` naming the command.

## Typewritten pages (text → PNG/PDF)

`-o` takes `.png`, `.svg` or `.pdf` (format by extension; anything else is an
error). SVG pages are large (~40 KB a character): prefer PNG or PDF.

```sh
pscat typewrite letter.txt -o letter.png
pscat typewrite --text "Dear Sir," -o short.png
pscat typewrite --reflow --spacing 2 --page-numbers bottom -o draft.pdf draft.txt
pscat typewrite --memo-to "All Staff" --memo-from "Records" \
    --memo-subject "Carbon copies" --memo-date "Oct 14, 1962" --height fit -o memo.png body.txt
```

The /Typewriter font: per-key quirks, carriage drift, per-strike
variation, ribbon ink; word wrap, tab stops, `--spacing`, `--indent`, a
memo header. Markup in the text: `<u>underlined</u>`, `<x>crossed
out</x>`, `<o>old|new</o>`, `<r>redacted</r>`. Same `--machine`/`--seed`,
same page. `--height fit` sizes the page to the text; longer text makes
more pages. Non-ASCII becomes ASCII or `?`. `--ps out.ps` writes a driver
that also runs in Ghostscript (`gs -dNOSAFER`). From PostScript:
`(lib/typewriter_outlines.ps) run (lib/typewriter.ps) run` then
`<< /Text (...) /Memo << /To (...) >> /Seed 3 >> tw-type showpage`.

A "released document" look (aged paper, scan skew, photocopy
generations, stamps, redaction bars, margin notes, staples, punch holes):
`lib/released.ps` after artkit and surfacekit (and handscript for notes):
`{ << /Text (...<r>name</r>...) /Redactor { rel-redact } >> tw-type } <<
/Age 0.6 /Generation 3 /Stamps [ << /Lines [ (RECEIVED) (14 MAR 1964) ] /X
462 /Y 700 >> ] >> rel-page showpage`. Every effect is optional, off at 0,
and seeded. Invent stamps and names; never reproduce real seals.

## Photo → engraving

```sh
pscat photo-etch photo.jpg -o etching.png      # or .svg / .pdf
pscat photo-etch --spacing 4 --angle 30 photo.jpg -o etch.svg
```

## Fonts

```sh
pscat --fonts                          # every findfont-reachable face
```

The LaserWriter 35 always resolve (Palatino, Bookman, AvantGarde,
ZapfChancery, NewCenturySchlbk, Symbol, ZapfDingbats, …). The ~35
display families (`/EBGaramond-Regular`, `/Bangers`, `/GreatVibes-Regular`,
…) and the Korean/Japanese/Thai faces come from the font catalog, which
a bare install may not have. **Unknown names substitute Helvetica rather
than erroring**, so check `--fonts` before relying on one. The Type 3
program faces load from the built-in library anywhere:
`(lib/fonts/neon.ps) run` (/Neon, /Marquee, /Constellation, /Lapidary,
/Circuitry, /Stitchwork, /Confetti).

## Capabilities (structured, for agents)

```sh
pscat --capabilities                   # fonts, palettes, templates, procedures as JSON
```

The machine-readable inventory of what this build actually has: fonts,
Type 3 faces, artkit's palettes, `lib/pagekit.ps`'s templates, and the
library's major procedures with their parameters, plus `pscat_version`.
Prefer it over guessing a name from prose (this file included).

## More skills

- **Art** (palettes, turtle, L-systems, brushes, paint and dry media,
  chalkboards, TeX on a board): the `psart` skill.
- **Charts and plots for papers** (`lib/dataviz.ps`, `lib/graph.ps`): the
  `psviz` skill.

## Translucency: setalpha / setblendmode (pscat extensions)

```postscript
0.3 setalpha              % fill/stroke opacity, 0..1
/Multiply setblendmode    % or /Normal (the default); nothing else
currentalpha currentblendmode
```

Graphics state (`gsave`/`grestore` save them, `initgraphics` resets
them). They reach fills, strokes, text and `shfill`, **not**
`image`/`imagemask`. Exported to SVG and PDF as well as PNG. **Not PLRM
operators**: Ghostscript has no PostScript-level alpha, so hand someone a
`--pdf` or `--png`, not a `.ps`, and probe with `systemdict /setalpha
known` in code that must run in both. Also pscat-only: `gstate .sameclip
bool` (is the clip in force the one the gstate captured).

## Watch it draw (needs a display)

```sh
pscat file.ps                          # live window; --speed N paces it
pscat -i                               # REPL + window: type, watch it draw
pscat --spool DIR                      # render every .ps that lands in DIR
```

Arrow keys browse completed pages in any window mode. `--spool` runs
whatever lands in `DIR`: only point it at a directory you trust.

## MCP

`pscat-mcp` (installed beside `pscat`) serves six tools over stdio:
`render_postscript` (source → PNG images inline, or SVG text, or a PDF
written to `out_path`; adds a `Lint:` block when the page looks wrong),
`eval_postscript` (what the program printed, or the error with the
offending command and line), `handwrite`, `chalkboard_math` (LaTeX → a
chalkboard PNG; above), `review_sheet` (the review sheet above, from inline
programs: byte for byte what `pscat sheet` writes) and
`describe_art_capabilities`.
Each call has a time limit (default 60 s) and the interpreter stops a runaway
program itself, with a `limit_exceeded` error whose `limit` field says `steps`,
`time` or `memory` (a render still returns the partial page); `stopped` cannot
catch it. `out_path` must be inside the
server's output root (its working directory unless registered with
`--out-root DIR`), end in the output format's extension (`.png`, `.svg`,
`.pdf`) and not sit in a dot-directory such as `.git/`, and large inline images are refused: lower `dpi` or
pass `out_path`. Register it with
`claude mcp add pscat -- pscat-mcp` (Claude Code) or
`codex mcp add pscat -- pscat-mcp` (Codex).

## Pitfalls

- **No `showpage`, no page** in Ghostscript. pscat's `--png` is
  forgiving (it writes trailing art), but add `showpage` to any file
  that should also work in gs.
- `--page WxH` is in points and sets the canvas; `--dpi` scales device
  resolution without changing coordinates. For a 2× PNG of a 612x792
  page use `--dpi 144`, not `--page 1224x1584`. Canvases are capped at
  8000×8000 device pixels.
- pscat is not a sandbox: a program can read any file you can read
  (`(path) (r) file`), and on the command line nothing stops an infinite
  loop unless you ask: `--max-steps N`, `--max-time SECS` and `--max-memory
  MIB` stop a runaway with a `limitexceeded` error. Run untrusted PostScript
  as you would an untrusted script.
- The interpreter targets Level 2 PostScript. If a construct misbehaves,
  compare against Ghostscript, the behavioural oracle.
- Files the docs call `examples/...`, `gallery/...` or `docs/...` are in
  the [repository](https://github.com/PSCatLabs/pscat),
  not in an install.
