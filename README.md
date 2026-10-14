# pscat

A PostScript interpreter, written in Rust, for watching PostScript programs
draw live — built for personal fun rather than as a Ghostscript
replacement, though it's meant to grow toward broad spec coverage over
time.

**[jeromebanks.github.io/postscript_interpreter](https://jeromebanks.github.io/postscript_interpreter/)**
— docs, the full gallery, a font specimen for every face, and a live
playground running the interpreter in your browser tab, no install.

## Status

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
sweeps unmodified), `--sweep NAME=` predefines `/NAME` in userdict for
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

## Building & running

Requires a stable Rust toolchain.

```sh
cargo run -- --page 500x500 examples/sierpinski.ps      # watch it draw, live
cargo run -- --speed 10 file.ps           # slower (steps per frame, default 100)
cargo run -- --png out.png file.ps        # headless render to PNG
cargo run -- --page 500x500 file.ps       # canvas size (default 612x792)
cargo run -- --spool jobs/                # act like a lab printer: render
                                          # each .ps/.eps dropped into jobs/
cargo run -- --halftone file.ps           # classic 45° halftone dots, like a
                                          # mono laser printer (window/PNG)
cargo run -- --lint --png out.png file.ps # self-check: blank page? unbalanced
                                          # gsave? stuff left on the stack?
cargo run -- --lint-strict file.ps        # ...and exit non-zero on any finding
                                          # (a run that errored is never "clean":
                                          # lint prints [run-error] and exits 1)
cargo run -- --selftest lib/paintkit.ps   # run a library's %%SelfTest blocks
./scripts/selftest.sh                     # every library's blocks, plus strict
                                          # lint over every rendering driver
cargo run -- file.ps --sweep-seed 1:12 \  # render 12 seeds, one grid PNG
    --contact-sheet grid.png
cargo run                                 # terminal REPL (headless canvas)
cargo run -- -i                           # REPL + live window: watch what you type draw
cargo run -- -i lib/handscript.ps         # ...with a library preloaded
cargo run -- -e '3 4 add ='               # evaluate a snippet
cargo test                                # language + pixel-level render tests
```

The `examples/` directory has three recursive fractals (Sierpinski
triangle, Koch snowflake, golden spiral), a found-file-style test card,
and a type specimen (`specimen.ps`); all render identically in pscat
and Ghostscript.

## Installing pscat without the repo

You don't need a git checkout or a Rust toolchain just to *use*
pscat. `scripts/package_release.sh` builds `pscat`/`pscat-mcp` and
bundles them with their runtime assets (`lib/`, `fonts/catalog/`,
`scripts/handwrite.sh`) into a standalone directory — CI does this
automatically and attaches the result to a
[GitHub Release](https://github.com/jeromebanks/postscript_interpreter/releases)
whenever a version tag is pushed (`.github/workflows/release.yml`).

To use one:

```sh
tar xzf pscat-<version>-<os>-<arch>.tar.gz
export PATH="$PWD/pscat-<version>-<os>-<arch>:$PATH"
pscat --png out.png some_file.ps
```

`lib/artkit.ps` and friends, and `fonts/catalog/`, resolve
automatically as long as they stay next to the `pscat` binary — the
whole point of the bundle layout. If you move the binary elsewhere,
set `PSCAT_ROOT` to wherever the bundle ended up (see `src/paths.rs`
for the full resolution order, which also covers running from a
`cargo build --release` checkout with no configuration, as before).

## Handwrite: string in, handwritten PNG out

```sh
./scripts/handwrite.sh "meet me at the bandshell at nine"
./scripts/handwrite.sh --size 44 --paper ruled --ink 0.5,0.1,0.1 \
    --seed 7 -o note.png "buy milk
call the bank about the thing"
```

Renders any string in the /HandScript dynamic font (Stage 12),
word-wrapped across lines the way a person fills a page — every
glyph is generated fresh, so repeated letters never match. The page
height is auto-sized to the text; newlines force line breaks, blank
lines are kept, and input is lowercased (the font has no capitals).

Options: `--size`, `--width`, `--height`, `--margin`, `--leading`
(line spacing × size), `--jitter` (0 = a calm hand), `--pen`,
`--ink R,G,B`, `--paper plain|ruled`, `--seed` (same seed, same
page), `--dpi`, `--halftone`, `--transparent`, `-o out.png`. Run with `--help` for
the full list.

The business logic lives in `lib/handscript.ps` — the font plus a
dict-driven layout API (`hs-write` draws, `hs-linecount` measures;
the script auto-sizes pages by running the count headlessly first).
The library draws nothing on load and runs unchanged in
Ghostscript, so other applications can embed the file wholesale;
the options-dict schema is documented in its header.

## The font library

`lib/fonts/` — original display faces, each one a self-contained
pure-PostScript file that defines a Type 3 font and draws nothing on
load (the `lib/handscript.ps` doctrine: embeddable wholesale, runs
unchanged in Ghostscript). Because Type 3 glyphs are programs, the
letterforms do things outline fonts can't:

| Face | Material |
|---|---|
| `/Neon` | Bent glass tube — the glow is five widening strokes of overdraw, no alpha anywhere |
| `/Marquee` | Theater-sign bulbs along every stroke, halos, glints, and the occasional burnt-out bulb (seeded rand) |
| `/Constellation` | Letters as star charts: magnitudes and tints rand-drawn, hairline asterism lines, field stars |
| `/Lapidary` | Chiseled capitals: shadow, raised face, dark incision, and a hairline of sunlight on the arris |
| `/Circuitry` | Copper PCB runs — solder-mask channel, specular seam, through-hole pads at every terminal, vias at pitch |
| `/Stitchwork` | Cross-stitch X's pinned to the 45° aida grid, three passes of floss, a seeded-jitter hand |
| `/Confetti` | A thrown handful — paper slips and dots in a six-color party palette, still falling |

All seven share one capital skeleton set (both cases map to the same
capitals; digits and `.,-'!?` included) and take dials through their
scratch dicts — retube `/Neon` in pink, dial `/Marquee`'s burnout
rate, warm up `/Lapidary`'s stone, re-spool `/Stitchwork` in blue
floss. `examples/font_library.ps` and `examples/font_library2.ps`
are the specimen posters:

<p><img src="lib/fonts/specimen.png" width="45%" alt="The pscat font library specimen"/>
<img src="lib/fonts/specimen2.png" width="45%" alt="The second folio"/></p>

```sh
cargo run --release -- --png poster.png examples/font_library.ps
cargo run --release -- --png poster2.png examples/font_library2.ps
```

## The font catalog

`fonts/catalog/` — 60 libre outline faces loaded from disk at
`findfont` time (never compiled in; the binary and wasm stay lean).
With it, **every name in the classic LaserWriter 35 resolves to a
metric-compatible libre face**: the bundled Liberation faces cover
Helvetica/Times/Courier, TeX Gyre covers Palatino, Bookman, New
Century Schoolbook, Avant Garde, Zapf Chancery, and
Helvetica-Narrow, and the URW symbol faces give `/Symbol` and
`/ZapfDingbats` real glyphs with their proper PLRM encodings. On
top of that: 35 curated display and text families — Garamond to
Playfair, Poppins to Oswald, Great Vibes to Permanent Marker,
fraktur, western, horror, arcade, terminal, sci-fi, stencil, comic.

```sh
cargo run -- --fonts                 # list every reachable face and alias
cargo run -- -e '/Palatino-Roman findfont 24 scalefont setfont ...'
cargo run -- --png sheets.png examples/font_catalog.ps   # the specimen sheets
```

`fonts/catalog/README.md` is the manifest (families, genres,
licenses — all OFL, Apache 2.0, GUST, or AGPL-with-font-exception;
per-family license files ride alongside the fonts). Drop your own
`.ttf`/`.otf` into any subdirectory and `/<FileStem> findfont`
finds it. The [font gallery](https://jeromebanks.github.io/postscript_interpreter/fonts.html)
on the project site shows every face — Type 3 library included — one
card at a time.

## Making art

`lib/artkit.ps` is the generative-art toolkit: seeded random helpers,
eight mood palettes with color mixing, turtle graphics with a pose
stack, an L-system engine, shapes, layout and text helpers — and the
`pathforall`-powered brushes: `alongpath` stamps anything at even
arc-length along any path (`charpath` text included), and `pathtext`
sets type along a curve, each glyph rotated to the tangent — `ctext`
and `ctextctr` build on `pathtext` for the circular case specifically
(seals, coins, "ring of type" compositions), the latter centering the
string on the circle automatically. `walkpath` is a richer centerline
sampler for the same brush use case: alongside the stamp's position
and tangent it reports normalized progress along the current subpath,
the arc-length spacing since the last stop, and start/end flags —
including a guaranteed call at each subpath's literal endpoint, which
`alongpath`'s plain pitch-stepping can't promise — the foundation
painterly brushes (variable-width ribbons, tapered ends) build on. A tiling section adds a general
`lattice` walk, `hex`/`tri` shape primitives, `hexgrid`/`trigrid`
drivers for the two other regular tessellations, and `truchet` — a
single motif at a random quarter-turn per cell, the classic trick that
turns a regular grid into a flowing, maze-like pattern (see gallery's
Woven Labyrinth). A hyperbolic-geometry section models the Poincaré
disk: `hpoint`/`hpolar` place points by hyperbolic radius and angle,
`horthocircle`/`hgeo`/`hpoly` build individual geodesics and closed
geodesic polygons (circular arcs orthogonal to the unit disk, or
diameters through the origin), `hreflect` reflects across one (circle
inversion), and `httile` runs the reflections breadth-first to
generate a regular `{p,q}` tessellation of the whole disk from one
fundamental polygon (see gallery's Infinite Descent, a `{7,3}`
tiling). A fractals section adds `edgefractal`/`edgepoly` — a
Koch-style edge-replacement curve generalized to any turn-delta
generator (presets `/koch` and `/quadkoch` in `FractalGens`, retrieved
with `fgen`), walked around any closed polygon — and `gasket`/`carpet`,
Sierpinski-style recursive area subdivision for a triangle and a
square (see gallery's Recursive Peaks, gasket-faceted mountains under
a carpet-driven starfield). A noise/flow-fields section adds `noise2`
(2D gradient/Perlin coherent noise off a `srand`-shuffled permutation
table), `curl2` (turns any scalar field into a unit-vector flow by
taking its normalized perpendicular gradient), and `advect` (traces a
particle through a vector field as a sequence of `lineto`s) — see gallery's
Lodestone, iron filings curling around a magnetic stone. A scatter
section is the area-shaped counterpart to `alongpath`: `screct` and
`scpath` build a region from a rectangle or from the current path
(flattened and closed the way `fill` sees it — `clippath scpath`
captures the clip region), `scin` and `scarea` answer containment and
area, and `scatter` places a caller-supplied mark across the region by
fixed count or by density, with a `/Weight` procedure for non-uniform
distributions, seeded scale and rotation variation, exact
`/MinSpacing` enforcement, and a hard deposit budget (see gallery's
Firefly Census, a night meadow in which every mark on the page is
scattered rather than placed by hand). All of it
deterministic under `srand`, all of it running unchanged in
Ghostscript.

```postscript
(lib/artkit.ps) run
newpath 100 400 90 thome                        % a turtle...
(F) << (F) 0 get (F[+F]F[-F]F) >> 4 lsys        % ...an L-system...
4.4 22 ldraw stroke                             % ...a plant.
newpath 60 100 moveto 500 200 550 300 300 380 curveto
(text can walk along any path now) pathtext     % type on a curve
```

On top of the toolkit sit four **style packs** (`lib/styles/`), one
per aesthetic, each adding three palettes and a drawer of motifs:
`steampunk.ps` (gears, rivets, pipework, pressure gauges, riveted
plate frames), `psychedelic.ps` (sunburst rays, wobble-ring blobs,
spirals, kaleidoscope repeats, hue-wheel color), `scifi.ps`
(starfields, ringed planets, HUD chrome, hex shields, the synthwave
grid floor, and `glowstroke` to sell it all), and `toon.ps` (the
cel-cartoon look: `celfill` flats under fat ink, speech bubbles,
action bursts, speed lines, halftone `dotfill`, dripping title
slabs). Load artkit, then the pack; `examples/style_*.ps` are the
four specimen posters.

<p><img src="lib/styles/specimen_steampunk.png" width="23%" alt="Aether &amp; Brass — the steampunk specimen"/>
<img src="lib/styles/specimen_psychedelic.png" width="23%" alt="Turn On The Sun — the psychedelic specimen"/>
<img src="lib/styles/specimen_scifi.png" width="23%" alt="Outer Reaches — the sci-fi specimen"/>
<img src="lib/styles/specimen_toon.png" width="23%" alt="Splat! — the toon specimen"/></p>

`gallery/hortus.ps` is the worked example, and the `psart` skill
(`.claude/skills/psart/SKILL.md`) teaches the whole workflow to any
agent: the render-look-refine loop, the toolkit, the style packs,
type as material, and the composition habits that keep pieces good.

A sibling library, `lib/graph.ps`, covers mathematical plotting
instead of freehand generative art: `setframe` maps a data-space
domain onto a device-space viewport, `plotfn`/`plotparam`/`plotpolar`
sample a function/parametric curve/polar curve into the current path,
and `axes` draws a bordered, ticked frame around them. A 3D side adds
`setview` (an azimuth/elevation camera), `project3`, and `plotsurface`
— a `z=f(x,y)` wireframe mesh, row by row and column by column, with
no hidden-surface removal (a deliberate scope cut; see the gallery's
Ripple Range for the cheap per-row-occlusion trick that works for a
single swept height field without needing a general solver). Neither
library depends on the other. `examples/graphing.ps` is a four-quadrant
specimen sheet exercising all four drivers.

```postscript
(lib/graph.ps) run
0 -1.2 6.283185 1.2 60 60 480 360 setframe   % domain -> viewport
newpath 0 6.283185 200 { sin } plotfn stroke % y = sin(x)
newpath 5 4 6 axes stroke                    % border + ticks
```

A second sibling, `lib/dataviz.ps`, covers chart-drawing instead of
mathematical plotting: `setdvframe` maps a value domain and category
count onto a device viewport, and `barchart`/`linechart`/`areachart`
read it directly — a per-element color callback (`{i v -> r g b}`,
same shape as artkit's `grid`) lets every bar or wedge pick its own
color, called before that element's path is built so a misbehaving
callback can't silently drop it. `scatterchart` has its own continuous
2D frame (`setscatterframe`, the same 8-arg shape as `graph.ps`'s
`setframe`); `piechart` draws pie wedges or, given a nonzero inner
radius, donuts — clockwise from 12 o'clock, the usual chart-library
convention. `dvaxes` decorates the categorical frame with a bordered,
ticked frame the same way `graph.ps`'s `axes` does. Independent of
both `artkit.ps` and `graph.ps`. `examples/dataviz.ps` is a six-panel
specimen sheet; the gallery's Field Notes pairs a bar chart and a line
chart on one shared category axis (weekly sightings against a
temperature trend) alongside a species-mix donut.
For papers and documents, both libraries have plain, paper-ready counterparts distinct from the stylized gallery pieces: `examples/graph_paper.ps` (a single framed `plotfn` figure with axes and labels) and `examples/dataviz_paper.ps` (bar, line, pie and scatter panels on one white page). An interactive counterpart lives at `site/charts.html` (mirroring `site/playground.html` but scoped to these two libs — pick a preset, edit the data or function, see it render live), and agents have a dedicated skill at `.claude/skills/psviz/SKILL.md` (alongside `pscat`/`psart`) that teaches the calling conventions and copy-paste templates so a data-visualization request goes straight to `graph.ps`/`dataviz.ps` instead of the generative-art loop.


```postscript
(lib/dataviz.ps) run
0 10 60 60 480 360 setdvframe                  % value range -> viewport
newpath [3 7 2 9 5] 0.25 { pop dvcolor } barchart
newpath 5 4 6 dvaxes stroke                     % border + ticks
```

A third sibling, `lib/etching.ps` (issue #15), turns a photograph into
a line engraving — the reproduction technique old newspapers and books
actually used before halftone screens, not edge detection. `et-dims`
reads a JPEG's own SOF marker (segment-skipping, not byte-scanning) to
get its width/height/component count without decoding it; `et-draw`
opens the same file through `/DCTDecode` — a general filter in this
interpreter, so a PS program can `readstring` decoded samples directly
without going through the `image` operator — and hatches it: parallel
lines whose stroke width tracks local darkness, plus a perpendicular
crosshatch pass in the deep shadows. Both honor the file's own EXIF
Orientation tag (issue #56), so a portrait phone photo sizes and
renders right-side up rather than sideways. No new Rust code; the
interpreter already had every building block this needed. Draws with ordinary
`moveto`/`lineto`/`stroke`, so it renders to `--png`/`--svg`/`--pdf`
alike. `scripts/photo_etch.sh` wraps it end to end (photo path in, PNG/
SVG/PDF out — no PostScript required); `examples/etching_demo.ps` is a
specimen sheet.

```postscript
(lib/etching.ps) run
<< /Photo (photo.jpg) /PageWidth 320 /PageHeight 240 >> et-draw showpage
```

A fourth sibling, `lib/pagekit.ps` (issue #18) -- but, like the style
packs, it *does* depend on `artkit.ps`, leaning on `tfblock` for
paragraph flow, `showctr`/`rrect` for layout, and `Palettes`/`pal` for
color. Five parameterized document templates --
`pgcard`/`pgletter`/`pgcertificate`/`pginvitation`/`pgposter` -- each
take `x y w h dict pgNAME` (the usual bottom-left-plus-size
convention) and a content dict of optional keys (title, body copy,
signature, and a few template-specific fields), so an agent fills in a
message without making layout decisions itself; every template
returns whatever body text didn't fit, the same leftover contract
`tfblock` already establishes. Two new palettes, `/vellum` and
`/marigold`, join artkit's eight -- registered the same way a style
pack registers its own, since (checked directly, not assumed) not
every one of artkit's mood palettes actually runs dark to light.
`examples/template_*.ps` is one specimen per template.

```postscript
(lib/artkit.ps) run
(lib/pagekit.ps) run
40 40 532 500
<< /Awardee (Ada Lovelace) /Body (For contributions to computing.)
   /Presenter (C. Babbage) /Date (1843-10-12) >>
pgcertificate pop showpage
```

A fifth sibling, `lib/paintkit.ps` (issue #41), is the foundation for a
painterly-brush series: `pkribbon` treats the current path as a
centerline and fills a variable-width ribbon along it, built on
artkit's `walkpath` centerline sampler (issue #40). One dict-driven
entry point -- base width, a `{t -> mult}` pressure profile over
normalized path progress (three presets ship: constant, linear taper,
and a non-linear bell curve), start/end taper, round/flat/pointed
caps, and seeded edge jitter -- with color left to the caller, same as
any other artkit shape helper. Multiple subpaths each become their own
ribbon; closed subpaths fill as two concentric loops with no caps;
degenerate paths fall back to a dot or a no-op rather than erroring.
`examples/paintkit_demo.ps` is a specimen sheet of all three
centerline shapes, all three pressure profiles, all three cap styles,
and jitter. `pknib` (issue #42) is an angled-nib calligraphy preset
built on `pkribbon`: mark width tracks the angle between local path
direction and a fixed `/Angle`, widest perpendicular to the nib and
narrowing toward a `/MinWidth` floor parallel to it, composing with
`pkribbon`'s own pressure/taper/jitter. It requires a single open
subpath (one call per stroke). `examples/paintkit_nib_demo.ps` shows
the same path at several nib angles, corners, a direction reversal,
broad-edge lettering strokes, and a flourish. `pkdry` (issue #43) is a
dry-bristle brush, also built on `pkribbon`: a bounded family of thin
offset bristles scattered across the centerline, each broken into
ink/no-ink runs by a seeded two-state Markov chain (`/Load` the
resume-contact rate, `/Dropout` the lose-contact rate, both expressed
per one `/Width` of travel so the same numbers read as the same
dryness regardless of `/Pitch`) -- ranging from a mostly loaded stroke
to visibly broken dry-brush texture with no raster work, every dash
its own small `pkribbon` call. `/Bristles` is hard-capped at 1..100,
and bristle count times raw sample count is checked against a fixed
deposit budget before any drawing starts, the two safety limits
against an accidentally unbounded render. `examples/paintkit_dry_demo.ps`
shows loaded/medium-dry/very-dry presets, bristle count/spread/width-
jitter variation, small per-bristle color variation, and a flourish.
`pkspray` (issue #44) is a spray-paint brush -- the one preset not
built on `pkribbon`, since spray is discrete particle deposition rather
than an offset band: seeded opaque particles scattered around each
sampled centerline stop under a radial falloff (three discrete levels,
min-of-m-uniforms draws, no pow/exp), an optional overspray mist
escaping past the nozzle edge, per-particle size variation, and
optional trigger-dwell bursts pooling particles at each subpath's
ends. Total deposits track arc length (about `/Density` per
nozzle-diameter of travel) regardless of `/Pitch`, and are bounded by
a deposit-budget safety limit checked during the counting pass, before
any drawing. Stencils need no library support: particles are plain
fills, so any active clip (`charpath` + `clip` included) masks them.
`examples/paintkit_spray_demo.ps` shows the same cloud at `/Speck` 3
(coarse bar) and `/Speck` 1.2 (fused vapour), a clean pass, a wide aerosol
pass, heavy overspray, the falloff levels, a word sprayed through a
charpath stencil, a star sprayed through an arbitrary-path stencil,
and a tag mark with bursts pooled at both ends.

`pktrowel` (issue #111) is the trowel / palette knife, and the one
preset in the file that is deliberately **not** built on `pkribbon`.
Every other preset here offsets a band around the path *normal*, which
is what a tuft of bristles does; a palette knife holds its own angle
independently of where it is going, so the deposit has to be swept
along the *blade* direction instead — geometry `pkribbon` structurally
can't express. The blade is divided into `/Lanes` bands across its
length, each running its own contact chain, and each run of contact is
filled as one strip. It lays **no solid base pass** — `pkoil` starts
with a full-width ribbon and puts ridges on top of it, whereas a trowel
deposits broken masses straight onto whatever is underneath, which is
what makes the underlayer keep reading through however many strokes go
over it.

The four controls that all sound like "less paint" are separated onto
genuinely different axes: `/Cover` (called `/Load` before issue #141,
still accepted as a deprecated alias) scales the deposit's *width* (a
nearly empty blade lays a narrow band), `/Coverage` is the contact
chain's steady state — holes *within* that width — `/Viscosity` is its
*rate*, trading many short chattering runs for few long chunky ones,
and `/Scrape` is static lengthwise thinning that narrows every lane and
kills some outright, the bare tracks a nicked blade leaves. `/Angle`
rakes the blade (footprint thickness goes as its cosine, so the guard
stops at ±80° rather than a silently blank 90°), `/Drag` smears each
run past where the blade lifted, and `/EdgeBuildup` piles the darker
ridges a blade leaves at its two ends. Deterministic under the caller's
own `srand`, with the random draws structurally fixed per lane and stop
so turning one control doesn't re-roll another's texture; bounded by
`Lanes * stops` against the same budget `pkoil` uses.
`examples/paintkit_trowel_demo.ps` is the specimen sheet — including
the same path set twice, once in `pktrowel` and once in `pkoil`, for
the contrast.

`pktrowelplane` (issue #180) is the palette knife's other job: laying a
*plane*, an area of a mountain face or a rock covered with overlapping blade
pulls so it reads as one flat facet that carries the blade's texture. The
current path is the area; `pktrowelplane` runs `pktrowel` along parallel
rows across it, in `/Direction` degrees, with every `pktrowel` key passed
straight through (`/Width`, `/Angle`, `/Coverage`, `/Scrape`, `/EdgeBuildup`,
`/Colors`, `/Light`, `/Touch`, ...). The edge is crisp because the path is the
clip, not because of the blade's geometry; each row overruns the area so the
blade's trim, drag and skew fall outside it. Rows are `Width * cos(Angle) *
(0.35 + 0.65 * Cover) * (1 - Overlap)` apart, and `/Overlap` unset follows
`/Coverage` (a patchier blade overlaps more); a tapering `/Pressure` narrows
the band, so the rows are spaced for the thinnest it gets inside the area
(read at 17 points, capped at 1, floored at 0.25) and a taper leaves no gaps
between rows (issue #362); `/RowJitter` and `/AngleJitter`
keep the rows from being machine-parallel. Above about `/Overlap 0.5` the rows
also cover each other's `/Scrape` nicks. Deterministic under the caller's own
`srand`, at most 4000 rows, the path left as the caller had it. It works
under any CTM and in Ghostscript. `examples/paintkit_plane_demo.ps`
(`docs/paintkit_plane_demo.png`) builds a mountain from planes in pigment
colours and sets out the direction and overlap.

`pkfan` (issue #114) is the fan brush: foliage, evergreen branches,
grasses, cloud feathering, soft texture. Its nearest relative is
`pkdry`, and exactly one term separates them. `pkdry` holds each bristle
at a *fixed* lateral offset, so its bristles run as parallel tracks; a
fan brush's bristles leave a flattened ferrule and **splay** apart as
the stroke travels, so each bristle's offset opens along the path. At
`/Splay 0` `pkfan` reproduces parallel bristles exactly — the specimen
sets the two side by side to make that the visible point.

The opening has a floor: `/Splay 1` narrows the ferrule to a quarter of
the tip's spread and never further. A version without it was rendered
first, and every bristle coincided near the start, turning the stroke's
first third into one opaque blob with a fan glued to the end. Real
ferrules are flat and wide. The easing was chosen the same way — linear,
`t^0.6` and `t^1.6` rendered side by side, with `t^0.6` the one that
reads as bristles springing apart rather than a smeared wedge.

The two mark families fall out of the same seeded contact chain `pkdry`
uses: many fine bristles at high `/Load` give the soft feathered mark,
few bristles at low `/Load` and high `/Dropout` give separated bristle
marks. `/Ragged` varies each bristle's length so the tips feather —
without it they all stop on one straight edge and the mark reads as a
comb — and `/Flick` tapers toward the tip, applied only to the run that
actually reaches it, so a bristle broken into four dashes doesn't become
four tapered commas. A single-point subpath is a *pressed* fan radiating
about the point, which is what makes foliage clusters;
`examples/paintkit_fan_demo.ps` builds conifer limbs, a grass bank and
shrub clusters out of exactly that.

`pkbroad` (issue #115) is the broad flat brush — the block-in tool for
sky and water bands, reflections and broad masses. Three presets here
already make a wide mark, so what earns it a place is that a brush
**runs out**: a loaded flat brush lays solid colour where it touches
down and goes progressively drier as it travels. `pkribbon` is a
geometric band with no tool character, `pknib` is a pen whose width
follows a fixed world angle, and `pktrowel` is a blade carrying a smear
rather than a reservoir — issue #111 left depletion out of it
deliberately for exactly this reason.

The band is divided into `/Grain` striations, each given a fixed
affinity for the surface, and the charge falls along the stroke as
`Charge * (1 - Depletion*t)`. A striation deposits while the charge
stays above its own affinity, so striations drop out one by one and the
band thins from solid to broken *along its own direction* — streaks,
not speckle. `/Depletion 0` turns the fade off for an even block-in; a
low `/Charge` skips from the start, the mark of a barely loaded brush.
Each striation's end tapers away when it runs out but keeps the brush's
square end when the stroke simply stops.

The controls are called `/Charge` and `/Depletion` rather than `/Load`
on purpose: `/Load` in this file is a contact rate per `/Width` of
travel (`pkdry`/`pkfan`/`pkoil`), and a paint reservoir draining along
the stroke is a different quantity. `/Angle` follows `pktrowel`'s reading —
brush rotation relative to travel, constant along a curve — not
`pknib`'s fixed world angle.

`pkbroad`, `pktrowel` and `pkfan` can carry more than one colour (issues #183,
#221, #224).
`/Colors [[r g b] ...]` lists 1 to 8 loaded colours from the brush's left
edge, looking along the travel, to its right; `/ColorMix` (0 for hard bands,
1 for a smooth gradient) blends neighbours; and `/Light` (degrees, as in
`pkdab`) puts the first colour on whichever side of each subpath faces the
light, so a stroke drawn backwards keeps its lit side. `pktrowel`'s
`/EdgeBuildup` ridges take their own edges' colours. `pkfan` takes the same
keys (issue #224): the colours run across its bristles, first colour on the
left of the travel, and a pressed fan paints them across its arc. `/Colors`
replaces the current colour, but `/ColorJitter` still varies it, and unset
keys change nothing.

`pkdab` takes `/Colors` and `/ColorMix` too (issue #226), but with no head to
load across, each dab takes one colour from the load, at the position its
existing outline-phase draw gives it: `[dark dark dark yellow]` is a mostly
dark mass with a few yellow dabs, and `/ColorMix 0` makes every dab one colour
outright. It takes no new random draw, so `/Colors` recolours a clump without
moving a dab. `/HighlightColor` and `/ShadowColor` on `pkdab` and `pkoil` are
the colours the lit and shaded layers mix toward instead of white and black
(`/Highlight` and `/Shadow` still say how far), so a sunlit bush can have
yellow-green highlights and blue shadows. All three are graded by an enclosing
`pkwet`.

`examples/paintkit_loading_demo.ps` is the specimen for all of the above (issue
#227): the same loaded evergreen brush-relative and then with `/Light 150`
(the first colour moves from the underside of the left branches to the upper
face of every one), a knife pull loaded white and blue, a broad brush loaded
ochre and teal at `/ColorMix` 0, 0.3 and 1, a bush and bank lip with and
without a warm `/HighlightColor` and cool `/ShadowColor`, and the loaded
evergreen's reflection pulled down by a directional `pkwet`. Draw a branch as a
real stroke out from the trunk when you want `/Light` to flip it: a chord
shorter than `/Pitch` (a pressed fan, a dab) is never flipped.

![The loading specimen](docs/paintkit_loading_demo.png)

**Pigments** (issue #185) give a brush a palette that mixes like paint instead
of like a lerp. `Pigments` is a named oil palette of thirteen colours
(`PigmentNames` lists them in tube order: `titaniumwhite`, `midnightblack`,
`phthaloblue`, `prussianblue`, `alizarin`, `sapgreen`, `cadmiumyellow`,
`indianyellow`, `yellowochre`, `vandykebrown`, `darksienna`, `burntumber`,
`brightred`), and `pigmix` takes name-and-weight pairs and returns `r g b` like
`mix3`:

```postscript
[ /phthaloblue 1 /cadmiumyellow 1 ] pigmix setrgbcolor   % green, not grey
[ /phthaloblue 1 /alizarin 1 ] pigmix setrgbcolor        % violet
[ /phthaloblue 1 /alizarin 1 /titaniumwhite 6 ] pigmix setrgbcolor  % lavender
/phthaloblue 0.25 pigtint setrgbcolor                    % 1 part white to 3
/sapgreen 0.1 pigshade setrgbcolor                       % a little black
```

Weights are parts, so `[ /a 3 /b 1 ]` and `[ /a 30 /b 10 ]` are the same
paint, and an entry may be a literal `[r g b]` array for any colour that is not
a pigment. The model is Kubelka-Munk's two-constant theory, run per r/g/b
channel (absorption over scatter, added by weight, then converted back to a
reflectance), with each pigment's tinting strength as its weight: a few
multiplies and three square roots per mix. Its limits are documented in
`lib/paintkit.ps` ("Pigments"): three bands, not a spectrum, so complements go
to a very dark neutral; mixed in encoded rather than linear rgb; a pot of paint,
not a glaze.

The presets take pigments through the colour keys they already have. A bare
pigment name works in `/Colors`, `/HighlightColor` and `/ShadowColor` as it
stands; `pigcolor` turns a pigment name or a `pigmix` spec into the `[r g b]`
those keys want, and `pigcolors` does a whole `/Colors` list that includes
specs (a spec written straight into a colour key is refused with a guard that
says to wrap it in `pigcolors`):

```postscript
<< /Colors [ /sapgreen /cadmiumyellow ] >> pkfan
<< /Colors [ /sapgreen [ /sapgreen 1 /cadmiumyellow 1 ] ] pigcolors >> pkfan
<< /HighlightColor /titaniumwhite /ShadowColor /prussianblue >> pkdab
<< /HighlightColor [ /cadmiumyellow 2 /sapgreen 1 ] pigcolor
   /ShadowColor /prussianblue pigcolor >> pkdab
```

`examples/paintkit_pigment_demo.ps` is the specimen: every pigment with six
tints beside the RGB lerp of the same recipe, three ramps (blue to yellow,
blue to alizarin, blue to white), eight classic landscape mixes, and the
palette driving a loaded broad brush, a bush and a fan-brush evergreen.

![The pigment specimen](docs/paintkit_pigment_demo.png)

`pkliner` (issue #116) is the liner/detail brush — the fine marks made
last and over everything else: bare branches, trunks, grass stems,
rigging, shoreline accents. The issue asks for the complement of
`pknib`, and that difference *is* the design: `pknib` is a rigid nib
whose width follows a fixed world angle, while a liner is a long soft
tip that holds far more paint than its width suggests. So the line
wanders off the centerline it was drawn from, whips from full width to a
point, and breaks into shortening dashes as the paint runs out.

The wander is the part nothing else in the file does. Every other preset
varies width or coverage *around* the path the caller drew — `pkribbon`'s
`/Jitter` roughens the ribbon's own edges, `pkdry`'s bristles ride at
fixed parallel offsets — but none of them leaves it. At 2pt the
centerline *is* the mark, which is the whole difference between a
painted branch and a ruled one.

Its two fades are deliberately independent: `/Taper` is the artist's
lift and changes width only; `/Charge` and `/Depletion` are the paint and
change coverage only. Coupling them would be defensible physically and
would make them inseparable in the artist's hands. The break-up comes
from a seeded surface "tooth" sampled at *absolute* spacing rather than
per-stroke, so a 300pt trunk and a 20pt twig don't come out
self-similar. Because that pool is fixed and emission draws nothing,
`pkliner` is the only seeded preset here that can promise a constant
number of random draws per subpath: retuning it re-shapes its own mark
instead of re-rolling everything drawn after it. A single-point subpath
presses a detail dot — one dab per `moveto`, and unlike `pkbroad`'s
pressed footprint it lands regardless of the tooth, because a pressed
tip makes contact by pressure.
`pkdab` (issue #117) is the foliage brush: clustered, irregular dabs for
bushes, leafy masses and vegetation texture. It is a pounce, not a drag,
so the current path is a *spine* — every stop seeds a cluster of blobby
dabs within `/Scatter` points (formerly `/Spread`, still accepted as a
deprecated alias) — and a single-point subpath is a standalone clump.
Density is per `/Size` of travel with a per-subpath accumulator (the
`pkspray` design), so `/Pitch` doesn't change coverage. Each dab is
generated once and painted in up to three layers from the same geometry:
a darker underlayer nudged away from `/Light`, the body in the current
colour, and smaller lighter dabs on the lit side. Re-deriving positions
per layer would scatter highlights off the mass they are meant to light.
`/Light` is a fixed user-space angle, like `pknib`'s `/Angle`, because
foliage is lit by the sun, not by the stroke direction. Every dab slot
draws a fixed 13 values whether or not it lands, so `/Charge`, `/Shadow`,
`/Highlight`, `/Light` and `/ColorJitter` re-shape the mark without
re-rolling it. Closed paths are not filled (no portable in-fill test); a
mass is built by pouncing along a spine through it.
`examples/paintkit_dab_demo.ps` is the specimen.

`pkwet` (issue #113) is the wet-on-wet softener, and the only entry
point in the file that draws no mark of its own. It takes the caller's
*mark-drawing procedure* and re-runs it, each pass displaced a little
further from the true path and mixed a little further toward `/Under` —
the color the artist declares is already on the canvas there — so the
mark's core stays solid while its edge grades away into the layer
beneath:

```postscript
{ newpath 40 400 moveto 200 430 380 390 560 420 curveto
  << /Nozzle 22 /Density 70 >> pkspray }
<< /Soft 0.8 /Under [0.62 0.72 0.86] >> pkwet
```

Being a wrapper rather than a `/Wet` key bolted onto each preset means
one implementation serves every stroke family in the file, and anything
added later. `/Soft` is the only knob that matters — it supplies `/Layers`
and `/Offset`, how far in points the outermost pass is displaced (named
`/Spread` before issue #141, still accepted as a deprecated alias) — and
at `/Soft 0` it marks the page exactly as calling the
procedure directly would, random stream included. Every pass runs inside
its own `gsave` and its own `stopped`, both of which the multi-pass case
needs, so a path or graphics state the procedure leaves *behind* does not
survive the call, and a `stopped` context is an `exit` boundary.

There is deliberately **no alpha** in it. The obvious implementation is
a translucent pass, and it was built and rendered before being
rejected: alpha applies to every fill the wrapped proc makes, and
`pkoil`'s ridges and `pktrowel`'s 18% seam-closing lane overlap then
composite against *themselves*, blotching the first and putting a dark
stripe at every lane boundary of the second — reintroducing the exact
artifact issue #111 removed. Grading opaque passes avoids that and, as
a bonus, needs nothing Ghostscript lacks, so `pkwet` has no
reduced-fidelity fallback path the way `pkwash` does.

Pickup is *declared*, not sampled: reading color back off the canvas
needs a pixel-sample operator PostScript doesn't have (issue #134), and
the artist knows what they just laid down anyway. Pair `pkwet` with a
broken or particulate brush — `pkspray`, `pkdry`, `pktrowel` at low
`/Coverage`. A solid brush keeps its own hard silhouette and merely
gains a halo; `examples/paintkit_wet_demo.ps` shows that comparison
side by side, along with a sky, clouds and two banks of mist. A
low-`/Coverage` mark is full of gaps, so mist laid on with one lets a
treeline read straight through it without any translucency at all.

With `/Direction` (issue #181) the same wrapper does the *directional*
wet-on-wet blends: the passes line up along one axis instead of
scattering round the mark, each a fixed step further along it with a
seeded wobble `/Stretch` times narrower across it. `/OneSided true`
keeps them all on one side, so `/Direction 270` drags a reflection down
the way a dry brush does, and `/Direction 90` lifts a mountain's foot
into haze; two-sided, `/Direction 0` sweeps a sky across. No pixel
readback is needed: the wrapper already re-runs the mark, so it re-runs
it along a line.

```postscript
{ gsave 0 2 WL mul translate 1 -1 scale  reflected-range  grestore }
<< /Direction 270 /OneSided true /Offset 22 /Under [0.44 0.54 0.66] >> pkwet
```

The wrapped procedure has to paint in the color it inherits (grading
sets the color before each pass) or in loaded colors (`/Colors` on
`pkbroad` or `pktrowel`, which pkwet grades by the same mix), so a
scene is wrapped element by element, and `/Direction` is measured in the user space pkwet is called
in — keep a reflection's mirror inside the procedure. A directional
call defaults to enough layers to keep the passes within 2pt of each
other (up to 16), because identical copies further apart read as
stepped echoes, so with `/Layers` left to default, `/Offset` also sets
the depth and re-rolls what follows; pin `/Layers` while tuning. Any
nest with a directional call in it may multiply its innermost
procedure at most 64 times — enough to pull a reflection down and then
graze it across. `examples/paintkit_wet_pull_demo.ps` shows a
reflection sharp, pulled and pulled-then-grazed, a mountain foot under
plain wet mist and under a lift, and a sky before and after a sweep.

`/Under` may also be a procedure `x y -> r g b` (issue #179) for a backdrop
that changes across the mark: a cloud over a graded sky, mist over a range
that fades into it. With one colour the outer passes are right at one end and
leave a pale or dark rim at the other. Give `/At [x y]` (where the mark sits,
in the user space you call pkwet in) and each softening pass asks the
procedure for the backdrop at `/At` plus its own displacement:

```postscript
{ ... cloud ... }
<< /Soft 1 /Offset 34 /At [135 70] /Under { exch pop skyat } >> pkwet
```

The procedure runs inside pkwet's frame, so it must be a pure function: no
painting, no `pkwet`, no reliance on the operand stack, and exactly three
numbers in 0..1 out (anything else is a named error). The question is per
pass, not per pixel, so a mark tall against the gradient is better split into
several calls, each with its own `/At`. A literal `[r g b]` behaves exactly
as before. `examples/paintkit_wet_under_demo.ps` sets one cloud over a strong
sky gradient with a constant and a procedural `/Under`
(`docs/paintkit_wet_under_demo.png`).

`pkcanvas` and `/Touch` (issue #182) give light strokes one canvas to
catch on. Every other "less paint" key in paintkit (`/Load`, `/Charge`,
`/Coverage`) models the tool, so two strokes that cross each break up on
their own. `opts pkcanvas` sets, once, a seeded weave of raised knuckles
(`/Seed` an integer, `/Weave` and `/Mottle` in canvas points, `/Relief` 0..1), and
`/Touch` (0..1) on `pktrowel`, `pkdry`, `pkfan`, `pkbroad` or `pkliner` lands
a mark only on the knuckles that pressure reaches: lower is lighter, 0
deposits nothing, 1 is full contact, identical to leaving it unset. Every
touched stroke catches on the same knuckles, so a dry brush and a knife
crossing share one weave:

```postscript
<< /Seed 7 /Weave 2.5 >> pkcanvas                 % once, first, before any translate
newpath 14 52 moveto 70 118 118 112 226 196 curveto
<< /Width 34 /Touch 0.7 >> pkdry                  % laid first, the heavier touch
newpath 18 206 moveto 90 172 150 84 224 40 curveto
<< /Width 32 /Touch 0.55 >> pktrowel              % the knife, on the same weave
```

Call `pkcanvas` before any `translate`, `scale` or `rotate` the piece
applies: the surface is defined in the space in force when it was set, and
a touched mark is clipped in that canvas space wherever it is painted
later. `/Touch` with no canvas is an error rather than a silent full mark (even
`/Touch 1`, which otherwise renders exactly as unset),
it takes no `rand` draws (so turning it on moves nothing painted after it),
and it composes with `/Colors` loads and with `pkwet`, inside which every
pass catches on the same knuckles. `examples/paintkit_canvas_demo.ps` shows
the crossing strokes full, on their own break-up and on the shared canvas,
a `/Touch` sweep, touched strokes with a `/Colors` load and a touched
stroke pulled down inside a directional `pkwet`.

![The canvas surface specimen](docs/paintkit_canvas_demo.png)

The presets above are brush *heads*; a painting is equally made of
brush *motions*, and the wet-on-wet vocabulary is mostly named ones.
Six path builders (issue #184) generate them, from a region or a guide
line plus a few artistic controls, and paint nothing: they leave an
ordinary path, so they feed any preset.

| Motion | Input | Builds | Classic brush |
|---|---|---|---|
| `pkcrisscross` | region | strokes at two crossing headings | `pkbroad` (sky) |
| `pkscumblein` | region | many small separate circles | `pkoil` (clouds) |
| `pkscumble` | the current path | a chain of overlapping loops | `pkdry` (a scrubbed edge) |
| `pktap` | the current path | short stabs along it | `pkfan`, or `/Length 0` for `pkdab` clumps |
| `pkpull` | the current path | strokes pulled from it along a direction | `pkbroad` down (reflection), `pkliner` up (grass) |
| `pkzigzag` | the current path | a zig-zag about it | `pkliner` (water) |

```postscript
0 0 700 160 screct << /Count 90 /Length 50 /Angle 25 >> pkcrisscross
<< /Width 30 /Charge 0.7 >> pkbroad                       % a sky block-in

newpath 0 300 moveto 800 300 lineto
<< /Direction -90 /Length 60 /Spacing 6 >> pkpull
<< /Width 5 /Charge 0.6 >> pkbroad                        % pulled down from a horizon
```

The region motions take an artkit region (`screct`, or `scpath` for any
shape) under their options dict; the line motions read the current path
as the guide, as `pkdab` does. Every motion *replaces* the current path,
so a guide, or the outline `scpath` leaves behind, is never painted. They
draw only from `frnd`, so they are deterministic under the caller's
`srand`, and each slot draws a fixed number of values, so changing
`/Length` or `/Angle` does not move where the strokes land. A region
motion lays a jittered grid sized from the region's area, so even
coverage is kept and `/Count` stays about right for a non-rectangular
region. `examples/paintkit_motion_demo.ps` shows each beside its brush.

All of these come together in the gallery piece *Alpine Lake*
(`gallery/alpine_lake.ps`, issue #118), a full wet-on-wet landscape
painted from paintkit's brushes alone. It has a `pkcrisscross` sky
painted with `pkbroad`, `pkspray` clouds and mist under `pkwet`, and
`pktrowel` snow and shadow planes on two ranges, the near one's foot
lifted into mist. A `pkdab`-crested forest sits on the far shore, with
`pkfan` evergreens, a pulled-down reflection and `pkliner` details.
The reflection is not a second drawing: every scene element seeds its
own randomness and logs it as it paints, so the lake replays the same
marks under `1 -1 scale`, one single-colour part at a time, each
pulled down with directional `pkwet` (issue #191).

How to paint each element of a landscape with these tools is written up as
a cookbook, [`docs/guides/landscape-cookbook.md`](docs/guides/landscape-cookbook.md)
(issue #186): eleven short recipes (sky, clouds, mountains, mist, far forest,
evergreens, bushes, water, reflections, foreground bank, grass), each with the
tools, the order, the settings that matter, the usual failure with its fix, a
complete snippet and a rendered thumbnail. `tests/landscape_cookbook.rs` runs
every snippet, so they keep rendering.

`pkwash` (issue #47) is the watercolor medium, and the one preset here
that needs something from the interpreter rather than only from
PostScript: it fills the current path as a *translucent* wash. Two new
operators back it — `setalpha`/`currentalpha` and `setblendmode`/
`currentblendmode` (`/Normal` or `/Multiply`), pscat extensions rather
than PLRM operators, living on the graphics state so `gsave`/`grestore`
snapshot them like any other paint attribute, and exported by `--svg`
(`fill-opacity`/`stroke-opacity`, `mix-blend-mode`) and `--pdf` (an
`ExtGState`'s `ca`/`CA`/`BM`) as well as `--png`. They reach fills,
strokes, text and `shfill`; they do **not** reach `image`/`imagemask`,
which blit their own samples. The wash itself is still pure vector
geometry: `/Layers` translucent passes, each with its own boundary
wobbled off the given path by `/Wet` (a harmonic ladder at integer
multiples of path progress, so a closed boundary joins seamlessly),
`/Bloom` edge pooling drawn as a stroke clipped to the wash so it
can't leak outside, and optional `/Grain` granulation — no diffusion
solver anywhere. `/Blend /Multiply` makes wash order commute, which
plain source-over does not. `pkpaper` lays the ground it sits on: tone
plus seeded grain and fibers. Both are deterministic under `/Seed`, or
under the caller's own `srand` without one.

This is also the first thing in this repo that does **not** render the
same way under plain `gs file.ps`: Ghostscript has no
PostScript-callable alpha operator at all. Rather than error there,
`pkwash`/`pkpaper` probe for `setalpha` at load and fall back to
painting each mark in its flattened-over-white equivalent — legible,
but overlaps stop mixing. Verified alpha output goes through `--pdf`
instead, which gs's own PDF interpreter renders correctly.
`docs/WATERCOLOR.md` is the decision record behind all of it (issue
#46's architecture spike), `examples/paintkit_wash_demo.ps` is the
specimen sheet, and the gallery's First Rain is the piece.

```postscript
(lib/artkit.ps) run
(lib/paintkit.ps) run
newpath 40 40 moveto 60 120 260 120 280 40 curveto
<< /Width 24 /Pressure { pktaper } >> pkribbon showpage
```

```postscript
0 0 612 792 << /Grain 0.6 >> pkpaper            % the paper first
0.2 0.35 0.7 setrgbcolor
newpath 300 500 120 0 360 arc closepath
<< /Alpha 0.3 /Layers 3 /Wet 8 /Bloom 0.6 >> pkwash   % then the wash
```

A sixth sibling, `lib/hatchkit.ps` (issue #49), is a reusable hatching
and cross-hatching library: `hatch` fills whatever region is currently
clipped with a family of parallel line strokes, so the caller supplies
the region and any tone-driving callback rather than this library
doing image analysis or tone extraction (`lib/etching.ps` already
covers that end-to-end). It never reimplements point-in-polygon or
edge-crossing tests -- lines are drawn well past the region's
boundary and left for the graphics state's own `clip` to cut down, so
`hatch` clips to concave and self-intersecting paths exactly as well
as convex ones. Single or multi-angle (`/Angles`, layered per call --
two calls at 90 degrees apart is the classic cross-hatch), seeded
width/wobble/dropout/length variation, and an optional per-sample
`/Density` callback that drives width buckets and a no-ink cutoff --
clamped into `[0,1]` regardless of what the callback returns, so a
misbehaving callback can only change tone, never spawn unbounded
geometry. `/MaxLines` and `/MaxSamples` reject a pathological
`/Spacing` before anything is drawn, the same up-front-reject shape
`scatter`'s `/Budget` uses. `examples/hatching.ps` is a three-panel
specimen sheet: flat shading, a `/Density`-driven tonal band that
reads as a curved, lit sphere despite every stroke being straight, and
layered cross-hatching built from two and three `hatch` calls over the
same region.

```postscript
(lib/hatchkit.ps) run
newpath 40 40 moveto 260 40 lineto 260 260 lineto 40 260 lineto closepath clip
<< /Angle 30 /Spacing 4 /Seed 1 >> hatch
```

An eighth sibling, `lib/halftonekit.ps` (issue #53), covers print
screens: `halftone` fills whatever region is currently clipped with a
regular dot, line, or cross-line lattice, one mark per cell sized by a
tone (a number, or a `{x y -> w}` callback clamped into `[0,1]`), with
a per-layer `/Offset` for controlled misregistration -- one call per
ink, each with its own color, angle, and shift, is a two-plate
risograph spread. Frequency is cells-per-inch, dots scale by
square-root (area-proportional, the print-correct curve), and a single
deterministic `/MaxCells` budget rejects pathological lattices before
anything is drawn. No random draws anywhere, so fixed options
reproduce identically with no seed to manage. `examples/halftone.ps`
is a four-panel specimen: a dot ramp, a line screen, a cross screen,
and a misregistered second plate.

```postscript
(lib/halftonekit.ps) run
newpath 40 40 moveto 200 40 lineto 200 200 lineto 40 200 lineto closepath clip
<< /Screen /dot /Frequency 9 /Tone 0.5 >> halftone
```

A ninth sibling, `lib/surfacekit.ps` (issue #51), is a library of
deterministic, seeded surface textures meant to sit under other marks
the way a real surface sits under ink: `grain` (paper grain), `fiber`
(paper fibers), and `scuff` (scratches and scuffs) are thin wrappers
over `scatter` (issue #48), the same "no new placement engine" choice
`lib/stipplekit.ps` made; `misreg` (print/registration imperfections)
is too, drawing nested lightening rings instead of a single mark;
`weave` (canvas weave) is its own grid-and-`scin` loop instead, since a
basket weave is a regular structure, not a random one, with its own
two independent pre-flight budgets mirroring `hatch`'s own
`/MaxLines`/`/MaxSamples` shape. New `/Color`/`/Strength` options lerp
each mark from paper-white toward a given color rather than toward
transparent (gs has no PostScript-callable alpha operator). A caller
needing an exact edge on a non-rectangular region clips the ambient
graphics state first, the same idiom `hatch` itself relies on.
`examples/surfacekit.ps` is a six-panel specimen sheet: one per preset,
plus a sixth demonstrating that exact-edge clip.

```postscript
(lib/artkit.ps) run
(lib/surfacekit.ps) run
0 0 200 200 screct << /Density 0.4 /Scale [0.4 1.4] /Seed 3 >> grain
```

A display-type sibling, `lib/lettering.ps` (issue #144), turns an
installed outline face into large "groovy" lettering: `psyletter`
takes `(text) x y << options >>` and draws the glyph outlines
(`charpath`) filled with a multicolor interior clipped to the letters --
`/Treatment` `/solid`, `/transition` (a `shfill` gradient), `/patches`
(seeded irregular color pools), or `/mottled` (patches plus worn
flecks) -- under a dark contour. `/Font` (default `/PermanentMarker`),
`/Size` or `/Width`, `/Align`, `/Palette` (an artkit palette name or an
array of `[r g b]`), `/Seed`, `/Wear`, `/Outline`, `/OutlineWidth` are
the controls. A seed changes only the texture, never metrics or
placement; a missing face is an error (`lettering-font-not-found`), not
a silent Helvetica. `psyletterpath` exposes just the geometry.
`examples/lettering.ps` is the specimen. With `lib/fillkit.ps` loaded,
`/Treatment` also takes `/plasma`, `/clouds`, `/diffclouds` and `/conical`
(see the next paragraph); `/Fill << ... >>` forwards any fillkit option.

`lib/fillkit.ps` (issue #154) reproduces GIMP's fill/render algorithms as
seeded fills over a box, meant to sit inside a clip: `plasmafill`
(diamond-square plasma; random RGB like GIMP, or blended through a
`/Palette`; `/Turbulence`), `solidnoisefill` (Perlin fBm, `/Turbulent`),
`diffcloudsfill` (|noise - noise|), and `gradshapefill` (`/bilinear`
`/square` `/conical` `/spiral`), all with `/RGBNoise`, `/HSVNoise` and
`/Spread` grain. Each is one small `colorimage` under the caller's clip, so
the shape stays vector in SVG/PDF; `/Resolution /device` samples once per device pixel (capped at 4096 per side; a
numeric `/Resolution` stays capped at 512).
`examples/fillkit.ps` is the specimen.

`lib/apparel.ps` (issue #146) builds apparel graphics on top of those two: one
config dict describes a front, back and optional left/right sleeve print area
(physical inches, DPI, margin, phrase, face, treatment, seed), and
The finished real-world config is `examples/keepin_it_real_shirt.ps` (design record: `docs/keepin_it_real_shirt.md`, exports in `art/keepin-it-real/`). `scripts/apparel_export.sh examples/apparel_shirt.ps out/` writes each piece
as PNG/SVG/PDF plus a proof sheet and `manifest.json`. Artwork is exported
with `--transparent` (PNG alpha, SVG without a white backdrop; PDF never had
one). `pscat --page 1400x760 examples/apparel_shirt.ps` is the proof sheet;
`examples/apparel_second.ps` shows a different design from configuration
alone.

```postscript
(lib/artkit.ps) run
(lib/lettering.ps) run
(Real) 306 300 << /Size 220 /Treatment /mottled /Seed 7 >> psyletter
```

A headline-composition sibling, `lib/headline.ps` (issue #145), lays out
one dominant run with smaller supporting runs tucked around it, from
*measured ink bounds* (not advance widths -- ascenders, descenders and a
rising capital are accounted for). `hllayout` takes a region and an
array of run dicts and returns a layout; `hldraw` draws it;
`hlrunink`, `hlcharink` and `hlink` read the measured boxes back. A run
is sized by `/Size`, `/FitWidth`, `/FitHeight`, `/FitBox`,
`/FitRegion` or `/SizeOf` (a ratio of another run), and positioned by
`/At` -- a point, or a point in an *earlier run's* ink box or one glyph
range of it -- plus `/Anchor` and `/Offset`. `/Text` may contain `\n`
line breaks (`/Justify`, `/Leading`). A run that would leave its region,
sit too close to what it `/Avoid`s (`/Clearance`), or fit smaller than
`/MinSize` raises a named error (`headline-run-outside-region`,
`headline-runs-collide`, `headline-fit-below-minimum-size`) instead of
overflowing silently. `/Draw` takes a per-line procedure, so
`psyletter` can render the same layout. `examples/headline.ps` is the
specimen.

```postscript
(lib/artkit.ps) run
(lib/headline.ps) run
[ 40 40 860 400 ] [
  << /Name /real /Text (Real) /Font /AlfaSlabOne /FitBox [ 780 300 ] /At [ 60 60 ] >>
  << /Name /keep /Text (Keepin' it) /Font /Helvetica-Bold /SizeOf [ /real 0.17 ]
     /At << /To /real /Chars [ 1 3 ] /H 0 /V 1 >> /Anchor [ /left /bottom ]
     /Offset [ 8 18 ] /Avoid [ [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ]
     /Clearance 10 >>
] hllayout hldraw
```

An arched-name-over-number sibling, `lib/jersey.ps` (issue #149), lays
out the classic athletic-jersey graphic: a team-like name on an arch above
a large player number, with an optional caption, fitted into a print
rectangle. `jerseylayout` takes an options dict (`/Name`, `/Number` as
text so `00` survives, `/Rect`, `/Margin`, fonts, `/Radius`, colours or a
`/Preset /light`/`/dark`) and returns a layout of measured *ink* boxes;
`jerseydraw` paints it (outline first, then fill) and `jersey` does both.
The arch is placed by glyph midpoint with the ink centred on the axis
(`jerseyarch` is the reusable path builder); the name is sized down until
its end letters stay within `/MaxTilt` and the number stays dominant, and
impossible geometry raises a named error (`jersey-name-too-wide`,
`jersey-name-too-tilted`, `jersey-does-not-fit`, ...) instead of clipping.
`examples/jersey.ps` is the specimen. Team-jersey lettering is also
available: `/Outline2` adds a second outer outline band (fill, thin accent,
dark outer), `/Shadow` a solid block shadow, `/Tracking` wide-tracked
capitals and `/NameWidth` a name narrower than the number;
`gallery/varsity_backs.ps` shows them on two jersey backs.

```postscript
(lib/artkit.ps) run
(lib/jersey.ps) run
<< /Name (Cleveland Steamers) /Number (69)
   /NameFont /AlfaSlabOne /NumberFont /AlfaSlabOne
   /Rect [ 36 36 576 684 ] /Margin 12 /Preset /light >> jersey
```

A tenth sibling, `lib/printkit.ps` (issue #52), composes `hatch`,
`scatter`, and (optionally) `grain` into three printmaking presets --
`woodcut`, `linocut`, `engraving` -- over a shared options dict
(`/Scale`, `/Density`, `/Roughness`, `/Seed`, `/Color`, `/Budget`,
`/Paper`, `/Angle`). Unlike its `region opts NAME` siblings, each
preset takes the *current path* directly and manages its own `clip`
internally -- no pre-clip, no `screct`/`scpath` region object, needed
from the caller -- since rebuilding an exact `clip`-able path back out
of a stored region's flattened edge soup would be real machinery for
no benefit over the path the caller already has. `woodcut` is one
directional hatch pass (the grain) at high wobble/dropout/trim plus a
scatter of small jittered chip marks; `linocut` is one bold, low-wobble
pass plus a sparse scatter of a few large, more deliberate marks;
`engraving` is a single `hatch` call sweeping three angles 60 degrees
apart at fine spacing and minimal wobble -- no chip marks. `/Paper
true` layers a subtle `surfacekit` `grain` pass under the ink, inside
the same clip. `examples/printkit.ps` is a four-panel specimen sheet
(one per preset, plus a `/Paper true` panel); the gallery piece
*Nightfall, Three Cuts* uses all three in one scene, each silhouette
inked solid first and then cut a second time with a lighter `/Color`
for the moonlit-highlight look real relief printing gets from gouging
ink away.

```postscript
(lib/artkit.ps) run
(lib/hatchkit.ps) run
(lib/surfacekit.ps) run
(lib/printkit.ps) run
newpath 40 40 moveto 260 40 lineto 260 260 lineto 40 260 lineto closepath
<< /Seed 5 >> woodcut
```

`lib/drykit.ps` (issue #234; design in `docs/DRYMEDIA.md`) is the core of
the dry media: chalk, crayon and charcoal. It draws the current path as a
dry stick dragged across a toothed surface, and pigment lands only where
the stick's contact patch meets raised tooth.
- **`dksurface`** names the surface: slate, green board, newsprint, drawing
  paper or construction paper. Each is a `pkcanvas` tooth plus the
  surface's colour, so paintkit and drykit marks share one canvas.
- **`dkstroke`** draws in the current colour.
  - Light `/Touch` catches only the tooth's peaks. The knuckles are stretched
    along the direction of travel, so the gaps read as streaks.
  - `/Tilt` lays the stick on its side.
  - `/Wear` flattens the tip along a stroke.
  - `/Lanes`, `/Load` and `/Dropout` break it into striations.
  - `/Fallout` scatters loose particles beside it.
  - Tone builds in deposit levels, drawn faint-first.
- **`{ ... } dkbatch`** draws several `dkstroke` calls level-major, so
  crossing hatch layers stay dark where they cross.

drykit takes no draws from the caller's `rand` stream. Its surface and
tool-side randomness come from private seeded generators, and fallout goes
through a seeded `scatter`, which restores the stream. Adding a stroke
never moves another mark.
Fine tooth relies on Ghostscript's fill adjust being off, and drykit turns
it off itself. `examples/drykit_specimen.ps` shows one stick on all five
surfaces, at three pressures, at three tilts, with a wearing tip and with a
crossing.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
<< /Surface /greenboard >> dksurface
0.93 0.94 0.90 setrgbcolor
newpath 40 60 moveto 300 80 lineto
<< /Width 3 /Touch 0.7 >> dkstroke
```

`lib/drychalk.ps` (issue #235) is chalk on a board, built on drykit.
- **`dcboard`** (`x y w h opts`) dresses a board and makes it the surface
  for chalk. It draws:
  - the board's tone, black slate or green board;
  - a dust film that is never flat;
  - the last lecture's writing, erased (`/Ghost`, a path procedure);
  - eraser swipe arcs, darker finger wipes and lighter smudges;
  - dust settled along the ledge (`/Ledge`) and in the chalk tray
    (`/Tray`).
- **`dcchalk`** strokes the current path as chalk, one stroke per subpath.
  - Each stroke lands heavy, tires in the middle and lifts thin.
  - Its weight differs a little from its neighbours'.
  - Dots are pressed, and dust falls beside every stroke.
  - With `/Tilt`, the stick lies on its side for broad underlines and
    shading: two bright edges over a granular face.
- **`/name dcchalkcolor`** sets white or a soft yellow, pink, blue, green
  or orange.

The defaults were tuned against the photographs in `docs/drymedia/refs/`.
Real chalk reads about halfway from the board to white, not white.
`examples/drychalk_specimen.ps` shows `gallery/hundred_lines.ps`'s
pen-stroke chalk before and after, on both boards.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/drychalk.ps) run
40 40 520 300 << /Surface /slate /Ghost { (old notes) 60 250 << /Size 30 >> sgpath } >> dcboard
/white dcchalkcolor
newpath (fresh notes) 60 180 << /Size 32 >> sgpath << /Width 1.8 >> dcchalk
```

`lib/strokeglyphs.ps` (issue #236, the first slice of the dry-media epic
#232) gives single-stroke skeleton glyphs for board math as *paths*, not
ink: capitals, handscript's lowercase and digits, a math hand (curly x,
crossed z), Greek, operators and relations, and growing delimiters.
`sgpath` appends a string's jittered centerlines to the current path
(UTF-8 or an array of glyph names), `sgwidth` measures from the fixed
advances, `sggrow` builds a sum, product, integral, radical, paren,
bracket, brace or bar sized to a height and depth, and `sgmetrics` gives
the advance, box, italic correction and TeX math class. Jitter comes from
its own seeded generator, never the caller's `rand`, and
`examples/strokeglyphs.ps` is the specimen sheet. A character or glyph name
with no glyph raises `sg-missing-glyph:<char>` from `sgpath` and `sgwidth`
before anything is drawn (issue #350); `/Strict false` restores the old
advance-300-and-draw-nothing behaviour. Issue #339 added the board-math
symbols a lecture needs: `≡ ∼ ≃ ⊆ ⊃ ⊇ ↦ ⊥ ∝ ∴ ∘ ∗ ⋆ ∖ ⟨ ⟩ ‖ ℏ ℓ ι υ Υ`, the
double, triple and contour integrals `∬ ∭ ∮` (also `sggrow` symbols), and
superscript digits (`i²`, `x⁻¹`); `examples/strokeglyphs_board_math.ps`
is their specimen.

```postscript
(lib/handscript.ps) run
(lib/strokeglyphs.ps) run
newpath (∀ε>0 ∃δ>0) 40 300 << /Size 40 /Seed 7 >> sgpath
1.5 setlinewidth 1 setlinecap 1 setlinejoin stroke
```

`lib/mathkit.ps` (issue #237) lays out a subset of TeX math as those
skeletons with TeX's box model. The subset covers:
- scripts, `\frac`, `\sqrt[n]{}`;
- `\sum`, `\prod`, `\int`, `\iint`, `\iiint` and `\oint` with limits, and `\lim`;
- `\left( \right)` delimiters that grow to fit, including `\langle \rangle`
  and `\|`;
- relations and operators such as `\equiv \sim \subseteq \mapsto \perp
  \therefore \circ \ast`, and `\hbar \ell`;
- matrices and `cases`, accents and `\boxed`;
- Greek and operator names.

Anything outside the subset fails with an error that names it. The
layout is written with a lecturer's irregularity: sizes and gaps vary,
scripts sit a little high or low, fraction bars overshoot, and the
baseline slopes and drifts (`/Irregular`, where 0 is clean TeX).
- `mkpath` appends a formula's path and `mkmetrics` measures it.
- `mkalign` writes a derivation aligned on `=`. It returns a record per
  line, which `mkframe` and `mkunderline` follow along its slope.
- `mkarrow` points a side note at a term.

Every TeX backslash is written twice in a PostScript string.
`examples/mathkit_specimen.ps` is the specimen: the fixed test set, from
the quadratic formula to Maxwell's equations, and a derivation in chalk.

```postscript
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/mathkit.ps) run
newpath (\\int_{-\\infty}^{\\infty} e^{-x^2}\\,dx = \\sqrt{\\pi}) 40 300
  << /Size 36 /Seed 3 >> mkpath
1.6 setlinewidth 1 setlinecap 1 setlinejoin stroke
```

## The website

**[jeromebanks.github.io/postscript_interpreter](https://jeromebanks.github.io/postscript_interpreter/)**
— architecture and extending guides, the full art/font/example
gallery, a dedicated [font gallery](https://jeromebanks.github.io/postscript_interpreter/fonts.html)
(every Type 3 face and every catalog family, one card each), and a
live playground running the wasm interpreter, all in the browser, no
install. Published from this repo by `.github/workflows/pages.yml`
(GitHub Pages, "GitHub Actions" source) on every push to `main`.
Sources in `site/`; `./scripts/build_site.sh` assembles `_site/` for
local preview (`python3 -m http.server -d _site`) — it also renders
the font gallery cards (`scripts/build_font_gallery.sh`) and the
example/gallery stills fresh each time, so the site never drifts from
what the interpreter actually does.

## In the browser

The interpreter core compiles to WebAssembly, and `web/pscat.js` is
a dependency-free ES module that renders — and *executes* —
PostScript in a browser, including step-driven drawing into a
`<canvas>`: the live window, in a tab.

```sh
./scripts/build_wasm.sh              # → web/pscat.wasm (~5.5 MB; the bundled base fonts)
python3 -m http.server -d web        # open http://localhost:8000
```

```js
import { Pscat } from './pscat.js';
const ps = await Pscat.load();
ps.run('0 0 300 300 rectfill showpage');   // to completion...
ps.paintTo(canvas);

ps.begin(source);                          // ...or watch it draw:
const frame = () => {
  if (ps.step(300) === 1) requestAnimationFrame(frame);
  ps.paintTo(canvas);
};
requestAnimationFrame(frame);
```

`web/index.html` is a ready-made playground (editor, speed slider,
live canvas). Errors come back REPL-style via `ps.error` with the
standard PostScript error names. The wasm build has no window, no
filesystem, and no clock (`usertime`/`realtime` read 0 — the one
documented deviation). Works under node too — `tests/wasm.rs` drives
the real module through the same JS library.

## For agents

pscat is easy to drive from a coding agent — Claude Code, Codex,
OpenClaw, Hermes, or anything else. Two integration surfaces:

**Docs-reading agents**: `.claude/skills/pscat/SKILL.md` is the
one-page tool reference (Claude Code picks it up automatically as a
skill; it's plain markdown, so any agent can read it). `AGENTS.md`
points there too, which is where Codex looks first.

**MCP-wired agents**: `pscat-mcp` (built alongside `pscat`) is an
MCP server over stdio exposing four tools:

- `render_postscript` — source in; PNG image(s) back inline (or SVG
  text, or a PDF written to `out_path`), with page size, `dpi`, and
  `halftone` options. Errors still return the partial render. Also
  runs the self-check/lint pass (issue #17, `--lint` on the CLI) and
  appends a `Lint:` block when it finds something — a blank page, an
  unbalanced `gsave`, stuff left on the stack — so a whole class of
  silent failure shows up without eyeballing the PNG.
- `handwrite` — text in, handwritten-note PNG back (the
  `scripts/handwrite.sh` options: size, paper, ink, jitter, seed).
- `describe_art_capabilities` — the machine-readable catalog of what
  this build's creative toolkit actually has installed: fonts, Type 3
  program faces, mood palettes, page templates, and artkit/style-pack
  procedures (issue #39, `--capabilities` on the CLI). See
  CAPABILITIES.md.
- `eval_postscript` — run headlessly, get back what the program
  printed, or the standard error name plus an operand-stack
  post-mortem.

Register it:

```sh
cargo build --release
claude mcp add pscat -- $PWD/target/release/pscat-mcp     # Claude Code
codex mcp add pscat -- $PWD/target/release/pscat-mcp      # Codex
```

For OpenClaw, Hermes, or any other MCP client, the generic stdio
config is:

```json
{ "mcpServers": { "pscat": { "command": "/path/to/target/release/pscat-mcp" } } }
```

The server shells out to the `pscat` CLI (found next to it), so the
tools always match the CLI's behavior. `handwrite` also needs
`scripts/handwrite.sh` — resolved the same way as `pscat`/`lib`/
`fonts/catalog` (see "Installing pscat without the repo" above), so
a release bundle needs no extra setup; set `PSCAT_ROOT` only if
you've moved the binaries away from their `lib`/`fonts`/`scripts`
siblings.

## Gallery

`gallery/` holds generative-art programs written in pure PostScript for
this interpreter, each *within the operator set it had at the time*.
The six Stage 3 originals predate `rand`, `sethsbcolor`, and arrays, so
they carry their own linear-congruential random generator and HSB→RGB
converter as PostScript procedures; Ring of Type is built on Stage 6's
fonts. Everything is deterministic; change a `/seed` and a different
tree or fern grows.

| Piece | Technique |
|---|---|
| `golden_bloom.ps` | Phyllotaxis — 1,300 florets at the golden angle, √-spaced like a sunflower |
| `cathedral_rose.ps` | Maurer rose — straight chords walking r = sin 6θ in 71° / 97° strides weave lace |
| `ember_tree.ps` | Recursive branching from an LCG, banded dusk gradient, layered sun glow |
| `fern.ps` | Barnsley chaos game — 48,000 points over four affine maps, no outline drawn |
| `silk_waves.ps` | 66 threads displaced by two interfering sine fields |
| `frost_mandala.ps` | Six-fold circle recursion, 11° twist per generation — 1,555+ circles |
| `ring_of_type.ps` | (Stage 6) one sentence circling eleven shrinking rings, set glyph by glyph around a charpath ampersand |
| `hundred_lines.ps` | (Stage 10) the Stage 12 /HandScript dynamic font writing punishment lines on a chalkboard — same sentence nine times, no two letters alike |
| `hortus.ps` | (Stage 19) a herbarium plate: three L-system plants grown by turtle, blossoms stamped along each plant's own path with `pathforall` |

View them one at a time:

```sh
./gallery/show.sh          # step through the rendered PNGs
./gallery/show.sh --live   # watch each piece draw itself in a window
```

<p>
  <img src="gallery/renders/cathedral_rose.png" width="30%" alt="Cathedral Rose"/>
  <img src="gallery/renders/golden_bloom.png" width="30%" alt="Golden Bloom"/>
  <img src="gallery/renders/frost_mandala.png" width="30%" alt="Frost Mandala"/>
</p>
<p>
  <img src="gallery/renders/ember_tree.png" width="30%" alt="Ember Tree"/>
  <img src="gallery/renders/fern.png" width="30%" alt="Fern, After Barnsley"/>
  <img src="gallery/renders/silk_waves.png" width="30%" alt="Silk Waves"/>
</p>
<p>
  <img src="gallery/renders/hundred_lines.png" width="30%" alt="Hundred Lines"/>
</p>

Details, page sizes, and re-render instructions live in
`gallery/README.md`.

A REPL taste:

```
PS> 40 2 add =
42
PS> mark 1 2 3
PS<4> pstack
3
2
1
-mark-
```

More sample programs live in `examples/`.

## Why

I used to write raw PostScript by hand in college and send it straight to
a LaserWriter. This is a from-scratch Rust interpreter built to relive
that — watching a hand-written recursive PostScript program draw itself,
live, without depending on a decades-old C codebase to do it.

<!-- sdlcify:managed:start -->
## SDLC

See [`SDLC.md`](SDLC.md) for how this repo's development lifecycle
works (issue → branch → PR → review → merge → cleanup) and the
branch-protection/merge policy currently enforced on GitHub — generated
and kept in sync by the `sdlcify` skill.
<!-- sdlcify:managed:end -->
