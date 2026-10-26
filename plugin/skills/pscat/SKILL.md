---
name: pscat
description: Render, preview, and debug PostScript with the pscat interpreter — PNG/SVG/PDF out, lint, seed sweeps, handwriting-style notes, typewritten letters and memos, photo-to-engraving, watch-it-draw window, halftone mode. Use for any .ps/.eps file work, generating diagrams or art in PostScript, producing notes, cards and lettering in a natural, hand-drawn script, or typed documents, or when the pscat MCP tools are available.
---
<!-- Contributors: tests/skill_blocks.rs runs every postscript code fence under plugin/skills on a bare install (no font packs) and fails on any error or font-pack warning. A fragment that cannot run alone is fenced as "postscript partial". -->

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

`pscat --bands N --png out.png f.ps` (N is 1..64) renders on N threads, one
horizontal band each, and gives the same PNG bytes as a plain render. It is only
faster for a program that reads `/BandBBox [llx lly urx ury]` (also `/BandIndex`,
0 = top, and `/BandCount`) and skips work outside it, e.g. a costly `colorimage`
procedure that computes only those rows plus a margin: the `psart` skill has a
runnable one ("Heavy noise renders"). PNG only; it refuses `--svg`, `--pdf`,
`--lint`, sweeps, `--profile` and `--allow-write`. `--max-steps` and `--max-time`
apply to each band. Command line only: `pscat-mcp` has no bands.

`pscat --profile --headless f.ps` (or with `--png`) prints, on stderr, each
named procedure's call count and exclusive and inclusive wall time, slowest
exclusive first (`--profile-top N`, `--profile-json`). Use it to find where a
slow render went. It lists only named procedures: work in an anonymous data
procedure shows up as "0 of 0", so add `--profile-ops` for time per built-in
operator (`fbm2` against `mul`; about 3x slower) or move the work into a named
procedure. `--profile-graph` adds caller -> callee edges. Not with `--bands`.

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

**All of the standard 35 (the LaserWriter 35) are built in**, so a bare
install has them: Helvetica, Times and Courier; Palatino, Bookman,
NewCenturySchlbk, AvantGarde, ZapfChancery and Helvetica-Narrow; Symbol
and ZapfDingbats. Everything else is a font pack, installed with
`pscat fonts install` (all packs) or `--pack <name>`:

| Pack | Faces |
|---|---|
| `extras` | the display families (`/EBGaramond-Regular`, `/Bangers`, `/GreatVibes-Regular`, …), Courier Prime, Special Elite, JetBrains Mono |
| `cjk-thai` | the Korean, Japanese and Thai faces |

`pscat --fonts` lists what this install can reach. Plugin installs have no
packs, so **design with the standard 35** unless the user has run
`pscat fonts install`. A name that is not available never fails the
render. Two cases:

- **A name from a pack that is not installed** (`/Bangers`) falls back to
  a built-in face, keeping weight and slant: the monospaced extras
  (Courier Prime, Special Elite, JetBrains Mono) become Courier; the other
  `extras` faces and the Korean/Japanese/Thai faces become Helvetica,
  which cannot draw those scripts. pscat prints one line on stderr per
  name, `pscat: warning: font /Bangers is in the extras font pack, which
  is not installed; using Helvetica. Run ...` with the
  `pscat fonts install --pack extras` command, and the run exits 0. Treat
  that line as a defect in a sketch meant to work on a bare install.
- **A name nothing knows** (`/NoSuchFont`) becomes Helvetica with no
  warning, so check `--fonts` before relying on a face.

With the pack installed the real face is used and nothing changes. The Type
3 program faces load from the built-in library anywhere:
`(lib/fonts/neon.ps) run` (/Neon, /Marquee, /Constellation, /Lapidary,
/Circuitry, /Stitchwork, /Confetti).

## Capabilities (structured, for agents)

```sh
pscat --capabilities                   # fonts, palettes, templates, procedures, native pscat operators as JSON
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
them). They reach fills, strokes, text, `shfill` and, since #766,
`image`/`imagemask`/`colorimage` (an image at alpha 0.5 is half
see-through; `Multiply` darkens what is under it; a soft-masked
ImageType 3 multiplies its own mask by the alpha). Exported to SVG and
PDF as well as PNG. **Not PLRM
operators**: Ghostscript has no PostScript-level alpha, so hand someone a
`--pdf` or `--png`, not a `.ps`, and probe with `systemdict /setalpha
known` in code that must run in both. Also pscat-only: `gstate .sameclip
bool` (is the clip in force the one the gstate captured).

## Noise: gnoise2 / fbm2 / ... (pscat extensions)

```postscript partial
42 noiseseed              % own table; never touches rand (default: 0)
x y gnoise2 n             % gradient noise ~[-0.71,0.71]; x y z gnoise3 ~[-1,1]
x y vnoise2 v             % value noise ~[-0.996,0.996]; x y z vnoise3
x y gnoise2d n dx dy      % with its analytic gradient
x y 5 0.5 2 fbm2 v        % oct gain lacunarity; fbm3 vfbm2 vfbm3 ridged2 ridged3
```

Native, about as cheap as an empty loop step, and bit-identical on every
platform. `lib/noiseref.ps` (`nrgnoise2` ...) is the same algorithm in
PostScript for Ghostscript-bound code; probe with `systemdict /gnoise2
known`. artkit's `noise2` already runs natively.

## Watch it draw (needs a display)

```sh
pscat file.ps                          # live window; --speed N paces it
pscat -i                               # REPL + window: type, watch it draw
pscat --spool DIR                      # render every .ps that lands in DIR
```

Arrow keys browse completed pages in any window mode. `--spool` runs
whatever lands in `DIR`: only point it at a directory you trust.

## MCP

`pscat-mcp` (installed beside `pscat`) serves ten tools over stdio:
`render_postscript` (source → PNG images inline, or SVG text, or a PDF
written to `out_path`; adds a `Lint:` block when the page looks wrong),
`eval_postscript` (what the program printed, or the error with the
offending command and line), `handwrite`, `chalkboard_math` (LaTeX → a
chalkboard PNG; above), `review_sheet` (the review sheet above, from inline
programs: byte for byte what `pscat sheet` writes) and
`describe_art_capabilities`, plus render jobs for anything slow.
Each call has a time limit (60 s by default; whoever registers the server raises it with
`pscat-mcp --timeout SECS`, and the client's own tool timeout must allow it too). A render
that may take longer goes through **`start_render`** (same arguments as `render_postscript`,
returns a `job_id` at once), then **`render_status`** (`queued`, `running`, `done`, `failed`,
`cancelled`, with `pages_done`) until it is finished, then **`render_result`** (exactly what
`render_postscript` returns); **`cancel_render`** stops one. A job may run an hour by default
(`--job-timeout`), two at once (`--max-jobs`). Raise `--max-steps` (or `0`) too, or the step
limit stops a long render first. The
interpreter stops a runaway
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
  (`(path) (r) file`; the MCP tools confine reads to the working directory,
  the pscat data directories and `--allow-read DIR`), and on the command line nothing stops an infinite
  loop unless you ask: `--max-steps N`, `--max-time SECS` and `--max-memory
  MIB` stop a runaway with a `limitexceeded` error. Run untrusted PostScript
  as you would an untrusted script.
- Writing files is off by default. On the command line, `--allow-write DIR`
  (repeatable) lets a program write inside DIR with `(DIR/f) (w) file`,
  `(a) file`, `renamefile` and `deletefile`; anything outside is
  `invalidfileaccess`. `(%stdout) (w) file` needs no flag. The MCP tools
  never write from PostScript: use their `out_path` instead.
- To save or reload a big numeric array, use `array (path) writereals` and
  `(path) n readreals` (packed binary64; add `/f32` before the operator for
  binary32), not printing and `run`ning text: exact, and about 10x faster to
  read. Writing needs `--allow-write DIR` like `(w) file`. `(path) /all
  readreals` reads every value, `(path) 100 50 readreals` is 50 values from
  value 100, and `array file writereals` / `file n readreals` take an open
  file (`%stdout`, an `(a)` append file). With `pscat --allow-write out f.ps`:

  ```postscript partial
  [ 0 1 99 { 0.1 mul dup 5 0.5 2 fbm2 } for ] (out/field.bin) /f32 writereals
  (out/field.bin) /all /f32 readreals length =      % 100
  ```
- The interpreter targets Level 2 PostScript. If a construct misbehaves,
  compare against Ghostscript, the behavioural oracle.
- `cvs` (and `=`, `==`) on a real prints the shortest digits that read back
  exactly, up to 17 significant digits, so the text can be 24 characters
  (`-3.3333333333333334e-301`). Ghostscript prints six. Size the string for
  that (`30 string`), or round first. `0.38612390214204787 12 string cvs`
  is a `rangecheck` here; rounding it to `0.386` first and then printing
  with `30 string` gives `(0.386)`.
- Files the docs call `examples/...`, `gallery/...` or `docs/...` are in
  the [repository](https://github.com/PSCatLabs/pscat),
  not in an install.
