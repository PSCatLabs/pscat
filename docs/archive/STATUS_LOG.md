# Status log (archived README section)

This is the "Status" section of `README.md` as it stood before the README became
a short front door, moved here unchanged. It is a stage-by-stage tour of how
pscat was built, in the order it was built. It records history; for current
usage see the [guides](../guides/README.md).

**All twenty roadmap stages are complete** (2026-07-19): 356 tests
across 35 suites, clippy clean, golden-checked against Ghostscript.
What follows is the tour, roughly in the order it was built.

**Stages 1–4 (the core)**: the language (tokenizer, object model,
three-stack interpreter, stack/arithmetic operators, `def`,
dictionaries, `if`/`ifelse`, `for`/`repeat`/`loop`/`exit`, comparisons,
`bind`), the graphics engine (paths, `arc`/`arcn`, `fill`/`eofill`/
`stroke`/`strokepath`, `gsave`/`grestore`, colors and line attributes,
`translate`/`rotate`/`scale`), and a **live window** so you can watch
programs draw — or headless PNG rendering, cross-checked against
Ghostscript on the example fractals. Errors use the standard PostScript
error names and never panic on program input (fuzz-tested); runaway
recursion is a catchable `execstackoverflow`, and tail-recursive
programs run in constant execution-stack space.

**Stage 5 (run found PostScript)**: the idioms real-world `.ps` files
depend on — arrays and strings with true PLRM view semantics
(`get`/`put`/`getinterval`/`forall`/`aload`), dictionaries with
arbitrary keys and `<<>>` literals, type conversions (`cvi`/`cvs`/
`cvx`/…), catchable errors (`stopped`/`stop`/`$error`),
`search`/`token`, clipping, dash patterns, HSB/CMYK color, the full
matrix operator set, `//immediate` names, and ASCII85 strings.
`examples/testcard.ps` — written like a found file, shortcut prolog
and all — renders identically in pscat and Ghostscript.

**Stages 6–7 (text, files, images)**: text in three font technologies:

- **The standard base fonts** (Helvetica, Times, Courier families,
  backed by bundled metric-compatible Liberation faces) through the
  full operator set: `findfont`/`scalefont`/`makefont`/`setfont`/
  `selectfont`/`definefont`, `show`/`ashow`/`widthshow`/`awidthshow`/
  `kshow`, `stringwidth`, `charpath` (text as geometry), Standard and
  ISOLatin1 encodings with the PLRM re-encoding idiom.
- **Type 3 fonts** — glyphs that are programs: `BuildChar`/`BuildGlyph`
  run on the machine in sealed glyph contexts (see
  `examples/type3_ransom.ps`, where every glyph is invented fresh at
  show time), including bitmap fonts that stamp their raster with
  `imagemask`.
- **Type 1 fonts** — the real thing: `currentfile eexec`, encrypted
  charstrings via the `N RD <binary>` idiom, a hint-ignoring
  charstring interpreter with flex and seac. The test suite generates
  a complete PFA and Ghostscript accepts the identical bytes.

And the Stage 7 machinery underneath: **file objects** sharing one
read cursor with the scanner (`currentfile`, `read*`, `token`/`exec`
on files), **decode filters** (ASCIIHex, ASCII85, RunLength, Flate,
LZW, DCT/JPEG) as composable on-demand file layers, and **sampled
images** — `image`/`imagemask`/`colorimage` in both operand forms,
from string, file, filter-chain, or procedure data sources, through
the full CTM/clip pipeline. `examples/postcard.ps` shows it all
inline; `FONTS.md` has the font design writeup, `HANDOFF.md` the
state of the world.

**Stage 8 (VM fidelity)**: full `save`/`restore` semantics
(`VM.md` is the design writeup, pinned against Ghostscript), interned
names (fib 27 ~1.6× faster), Indexed/Separation color spaces, Level 2
odds and ends, and a found-file corpus — tiger.eps and the other gs
classics render block-identical to Ghostscript.

**Stage 9 (output targets)**: real multi-page `showpage`
(the window keeps showing the finished page; `--png` numbers pages),
`--dpi` for print-resolution rendering, and `--svg`/`--pdf` export —
both mirror the paint pipeline directly, and the PDF is verified by
letting Ghostscript rasterize it back. `--pdf` output is a real
document, not just a picture: `%%Title:`/`%%For:` DSC header comments
(issue #8) become the PDF's `/Info` metadata, so a multi-page piece
opens in a reader — Kindle's Send-to-Kindle and virtually any e-reader
or PDF viewer already take PDF directly, no separate export needed —
with an actual title instead of a filename. Level 3's `shfill`
operator (issue #20) paints axial and radial gradients — `ShadingType` 2/3 with
`FunctionType` 2/3 (exponential and stitching, so multi-stop ramps
work, not just two-color) — through the same three seams: real
tiny-skia gradients on the raster/window path, native
`<linearGradient>`/`<radialGradient>` in SVG, and a flat average-color
approximation in PDF (documented gap — real PDF shading needs pattern-
colorspace machinery this exporter doesn't have). `lib/artkit.ps`'s
`gradfn`/`axialsh`/`radialsh`/`gradfill` build shading dictionaries
from a plain array of `[r g b]` colors instead of requiring one by
hand; `examples/gradients.ps` is the specimen sheet.

`--sweep-seed`/`--sweep` (issue #21) render a file once per value in a
sweep instead of once, so exploring seeds or a parameter is one
invocation instead of N hand-edits: `--sweep-seed` overrides every
`srand` call transparently (found art with a hardcoded `N srand` line
sweeps unmodified) except a restore of a value saved with `rrand`, which
puts the saved stream back (issue #202), `--sweep NAME=` predefines `/NAME` in userdict for
a source that opts in to read it. `--png` writes numbered per-frame
files; `--contact-sheet PATH` composites every frame into one grid PNG
instead (`--grid COLSxROWS` overrides the default layout) — either or
both. `examples/sweep_demo.ps` is the specimen. CLI-only for now
(`pscat-mcp` doesn't expose a sweep tool — a documented scope cut, not
an oversight; see NOTES.md).

**Stage 10 (the LaserWriter experience)**: `--spool DIR`
turns the window into the printer in the corner of the lab — it
idles, watches a directory, and renders each `.ps`/`.eps` that lands
there, page by page, each job in a fresh interpreter. `--halftone`
screens the raster like a mono laser printer's RIP: classic
euclidean dots on a 45° lattice, coverage tracking darkness, for the
window and `--png`. Gallery II opened with *Hundred Lines*, the
Stage 12 handwriting font writing punishment lines on a chalkboard.

**Stage 11 (performance parity)**: `cargo bench --bench vs_gs` races
both interpreters on speed and peak memory. pscat starts 5× faster,
uses 3–5× less memory on every workload, and wins every rendering
page; the honest remaining gaps (fib ~2.3×, fern ~1.9×) and what was
done about them live in NOTES.md.

**Stage 12 (handwriting)**: `examples/handwriting.ps` — a Type 3
font whose glyphs are generated fresh per draw with rand-jittered
strokes, wandering baselines, and varying pen pressure. No two
letters ever match, every page is reproducible, and the same file
runs in Ghostscript.

Stages 13–20 each have their own section below: the handwrite tool,
agent integration (skill + MCP server), the Type 3 font library, the
browser/WASM build, the GitHub Pages site, the font catalog, the art
toolkit, and the style packs.

Try `cargo run -- --page 500x500 examples/sierpinski.ps` and watch the
triangles appear.

See `INIT.md` for the project vision, `ARCHITECTURE.md` for the design
writeup, `NOTES.md` and `notes/` for per-stage summaries, and `ROADMAP.md` for the
full staged plan (all stages complete, with per-task model routing).
