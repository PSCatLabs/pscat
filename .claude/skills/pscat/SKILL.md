---
name: pscat
description: Render, preview, and debug PostScript with the pscat interpreter — PNG/SVG/PDF out, handwritten-note generation, watch-it-draw window, printer-style spool and halftone modes. Use for any .ps/.eps file work, generating diagrams or art in PostScript, or producing human-looking handwritten notes.
---

# pscat — PostScript as a tool

`pscat` is a PostScript interpreter in this repo. Build once with
`cargo build --release`; the binary is `target/release/pscat`.
Everything below also works with `cargo run --release --` in place of
the binary.
Run it from the repo root, or set `PSCAT_HOME` to the repo: a release
binary run elsewhere uses the `lib/` compiled into it (stale if you edited
`lib/` since the build) and finds no font catalog, so catalog faces such as
`/Bangers` silently fall back to a substitute.

## Render a file (the common case)

```sh
pscat --png out.png file.ps            # headless render to PNG
pscat --svg out.svg file.ps            # vector SVG (one file per page)
pscat --pdf out.pdf file.ps            # multi-page PDF
pscat --page 500x500 --png out.png f.ps  # canvas in points (default 612x792)
pscat --dpi 300 --png out.png f.ps     # print resolution (default 72)
pscat --halftone --png out.png f.ps    # screen like a mono laser printer
generate | pscat --png out.png -       # `-` reads the program from stdin
```

Multi-page documents number their pages: `out.png` → `out-001.png`,
`out-002.png`, … SVG does the same; PDF is naturally multi-page.

## Sweep a seed or parameter (explore a design space in one run)

```sh
# Reseeds every `srand` call transparently -- works on found art
# unmodified, no source edit needed.
pscat file.ps --sweep-seed 1:12 --contact-sheet grid.png

# Predefines /NAME to each value in turn before running; the source
# opts in by reading it: `/NAME where { pop NAME } { default } ifelse`.
pscat file.ps --sweep Density=10,20,40,80 --contact-sheet grid.png
```

One sweep axis per run (`--sweep-seed` and `--sweep` are mutually
exclusive), `A:B` / `A:B:STEP` / `A,B,C` spec syntax, capped at 64
frames. Needs `--png` (numbered per-frame files) and/or
`--contact-sheet PATH` (one composited grid PNG; `--grid COLSxROWS`
overrides the default square-ish layout) — at least one is required.
A per-frame PostScript error doesn't abort the sweep (the failed
frame's partial canvas is still written, same philosophy as a normal
render's error handling); the process exits nonzero if any frame
failed. `examples/sweep_demo.ps` is a runnable specimen for both
mechanisms. This is the seed/parameter-exploration workflow — an
agent comparing N attempts without re-invoking the renderer by hand
per attempt, or hand-editing the source between tries.

## Debug a program

```sh
pscat --headless file.ps               # run without rendering; errors → stderr
pscat --headless --pstack-on-error f.ps  # + gs-style operand stack post-mortem
pscat -e '3 4 add ='                   # evaluate a snippet
pscat --lint --png out.png file.ps     # self-check the render (see below)
```

Exit code 0 = clean run; nonzero = a PostScript error, named on
stderr with the standard error names (`undefined`, `typecheck`, …),
now with an `; Line: N` when the error happened in the top-level
program (not inside a `run`-loaded library file). `--png` still
writes the partial canvas after an error — useful to see how far a
program got.

**`--lint`** catches mistakes a render can have without *erroring* —
the kind that only show up by eyeballing the PNG otherwise: a blank
page (nothing painted), an unbalanced `gsave`/`grestore`, or stuff
left on the operand/dict stack. Findings print to stderr as
`pscat: lint: [check] message` (or `pscat: lint: clean`); it doesn't
change the exit code — read the findings, don't just grep for
failure. A run that ended in a PostScript error is never `clean`: it
prints `pscat: lint: [run-error]` after the error report (and exits
non-zero, as any errored run does). `pscat-mcp`'s `render_postscript` runs this automatically
and appends a `Lint:` block to its response when there's something to
report (silent when clean).

## Handwritten notes (string → PNG)

```sh
./scripts/handwrite.sh "meet me at the bandshell at nine" -o note.png
./scripts/handwrite.sh --paper ruled --size 30 --seed 7 "text"
./scripts/handwrite.sh --ink 0.4,0.1,0.1 --jitter 24 "shakier, redder"
```

Renders text in the /HandScript dynamic font: rand-jittered strokes,
no two letters alike, reproducible per `--seed`. Word-wraps to the
page and auto-sizes the height. **Lowercase only** — input is
lowercased, and characters outside a–z 0–9 `.,-'!?` advance
invisibly. The underlying library `lib/handscript.ps` is embeddable
(one options dict; see its header for the schema).

## Typewritten pages (text file → PNG/PDF)

```sh
./scripts/typewrite.sh letter.txt -o letter.png
./scripts/typewrite.sh --reflow --spacing 2 --page-numbers bottom -o draft.pdf draft.txt
./scripts/typewrite.sh --memo-to "All Staff" --memo-from "Records" \
    --memo-subject "Carbon copies" --memo-date "Oct 14, 1962" --height fit -o memo.png body.txt
```

Types the text in the /Typewriter font: a machine's per-key quirks, each line's
carriage return and drift, per-strike variation and ribbon ink; word-wrapped,
with tab stops, `--spacing`, `--indent`, and an optional memo header. Markup in the
text: `<u>underlined</u>`, `<x>crossed out</x>`, `<o>old|new</o>`, `<r>redacted</r>` (a black bar). Same
`--machine`/`--seed`, same page, in pscat and Ghostscript (`--ps out.ps` writes the
driver for `gs -dNOSAFER`). `--height fit` sizes the page to the text; text past
a page makes more pages (PNG: `out-001.png`, ...). `--reflow` joins each
paragraph's lines. Non-ASCII becomes ASCII or `?`. From PostScript:
`<< /Text (...) /Memo << /To (...) >> /Seed 3 >> tw-type showpage` after
`(lib/typewriter_outlines.ps) run (lib/typewriter.ps) run`; `tw-linecount` and
`tw-pageheight` measure. `--help` lists the options.

A "released document" look (aged paper, scan skew, photocopy generations, stamps,
redaction bars, margin notes, staple and punch holes): `lib/released.ps` after
artkit, surfacekit (and handscript for notes). `{ << /Text (...<r>name</r>...)
/Redactor { rel-redact } >> tw-type } << /Age 0.6 /Generation 3 /Stamps [ << /Lines
[ (RECEIVED) (14 MAR 1964) ] /X 462 /Y 700 >> ] >> rel-page showpage`; every effect
is optional, off at 0, and seeded (`/Seed`). Stamps and names must be invented.

## Fonts

```sh
pscat --fonts                          # list every findfont-reachable face
```

The complete LaserWriter 35 resolves libre (Palatino, Bookman,
AvantGarde, ZapfChancery, NewCenturySchlbk, Helvetica-Narrow,
Symbol, ZapfDingbats included), and `fonts/catalog/` adds ~35
display/text families — `/EBGaramond-Regular`, `/Bangers`,
`/GreatVibes-Regular`, `/PressStart2P-Regular`, … (a bare family
name means its `-Regular` cut). `lib/fonts/` holds the Type 3
program-faces — /Neon, /Marquee, /Constellation, /Lapidary,
/Circuitry, /Stitchwork, /Confetti — load one with
`(lib/fonts/neon.ps) run` from the repo root. Unknown names
substitute Helvetica rather than erroring;
`examples/font_catalog.ps` renders the specimen sheets.

## Capabilities (structured, for agents)

```sh
pscat --capabilities                   # fonts, palettes, templates, procedures as JSON
```

The machine-readable inventory of everything this build actually has
installed — fonts, Type 3 faces, artkit's mood palettes,
`lib/pagekit.ps`'s templates, and artkit's/the style packs' major
procedures — with a `pscat_version` field for drift detection. Prefer
this over guessing a name from prose (this file included; see
docs/dev/CAPABILITIES.md). Same catalog is exposed to MCP clients as the
`describe_art_capabilities` tool.

## Making art

For generative pieces — palettes, turtle graphics, L-systems,
stamping along paths, type on a curve — load `lib/artkit.ps` and see
the `psart` skill (`.claude/skills/psart/SKILL.md`): it teaches the
whole render-look-refine workflow.

## Translucency: setalpha / setblendmode (pscat extensions)

```postscript
0.3 setalpha              % fill/stroke opacity, 0..1
/Multiply setblendmode    % or /Normal (the default); nothing else
currentalpha currentblendmode
```

Graphics state, so `gsave`/`grestore` snapshot them and `initgraphics`
resets them. They reach fills, strokes, shown text and `shfill` — **not**
`image`/`imagemask`, which paint opaque regardless. Exported by `--svg`
and `--pdf` as well as `--png`.

**These are not PLRM operators.** Ghostscript has no
PostScript-callable alpha operator at all, so a program that uses them
will not render the same under plain `gs file.ps`. Two consequences
worth planning around:

- To hand someone a verified translucent render, use `--pdf` (gs's own
  PDF interpreter does understand transparency) or `--png`, not a `.ps`
  file they will run through gs.
- Library code that wants to survive both should probe:
  `systemdict /setalpha known` — `lib/paintkit.ps` does exactly this
  (`pkalphaok`) and falls back to flattening each mark against white.

`lib/paintkit.ps`'s `pkwash`/`pkpaper` are the watercolor medium built
on top of these; `pscat --capabilities` has their full option list, and
`docs/dev/design/WATERCOLOR.md` explains why the mechanism is shaped this way.

One more pscat extension, not in Ghostscript: `gstate .sameclip bool` is
true when the clip in force is the very clip the gstate captured (both
none, or the same clip chain; a re-clip with the same path is false).
Probe it with `systemdict /.sameclip known`; `lib/drykit.ps` does, to see a
bound wrapper's `grestore` inside a `dkbatch` (`notes/488-sameclip.md`).

## Watch it draw (needs a display)

```sh
pscat file.ps                          # live window; --speed N paces it
pscat -i                               # REPL + window: type, watch it draw
pscat -i lib/handscript.ps             # ...with a library preloaded
pscat --spool DIR                      # render every .ps that lands in DIR
```

Arrow keys browse completed pages in any window mode.

## Pitfalls

- **No `showpage`, no page**: Ghostscript emits nothing for a
  program that never calls `showpage`. pscat's `--png` is forgiving
  (it writes trailing art), but add `showpage` to any file that
  should also work in gs.
- `--page WxH` is in points and sets the canvas; `--dpi` scales
  device resolution without changing coordinates. For a 2× PNG of a
  612x792 page use `--dpi 144`, not `--page 1224x1584`.
- The interpreter targets Level 2 PostScript. If a construct
  misbehaves, compare against `gs` — the repo treats gs as the
  behavioral oracle (see AGENTS.md).
- MCP alternative: `pscat-mcp` (same target dir) exposes
  `render_postscript`, `handwrite`, and `eval_postscript` as MCP
  tools over stdio, for agents wired that way. See
  `docs/guides/agents.md`.
- Browser/JS alternative: `./scripts/build_wasm.sh` then serve
  `web/` — `web/pscat.js` runs and renders PostScript in a page
  (`Pscat.load()`, `run`/`begin`/`step`, `paintTo(canvas)`);
  `web/index.html` is a ready playground. No filesystem or clock in
  the wasm build.
