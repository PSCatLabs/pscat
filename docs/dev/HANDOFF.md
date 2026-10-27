# HANDOFF.md — state of the interpreter and how to continue

> Where the docs live: user guides in [`docs/guides/`](../guides/README.md),
> contributor docs (this file and its neighbours, indexed in
> [`README.md`](README.md)) in `docs/dev/`, and history, including the frozen
> `NOTES.md`, `ROADMAP.md` and `INIT.md`, in [`docs/archive/`](../archive/README.md).
> Per-issue notes are in [`notes/`](../../notes/README.md).

Written 2026-07-13 at the completion of Stages 6–7; last updated
2026-08-15. Stages 8–20 are all complete: spool/halftone/Gallery II
(10), perf parity (11), handwriting (12), the handwrite tool (13),
agent integration — skill, `pscat-mcp`, `pscat -` (14), the
font library `lib/fonts/` — /Neon, /Marquee, /Constellation,
/Lapidary + specimen (15), the browser build — wasm + web/pscat.js (16), the Pages site + pipeline — site/, scripts/build_site.sh, .github/workflows/pages.yml (17), and the font catalog —
`fonts/catalog/` runtime loader completing the standard 35, plus
/Circuitry, /Stitchwork, /Confetti (18), the art toolkit —
pathforall/flattenpath, lib/artkit.ps, the psart skill, Hortus
Machinalis (19), and the style packs — `lib/styles/`
steampunk/psychedelic/scifi/toon motif libraries over artkit, with
specimen posters in `examples/style_*.ps` (20) — plus
`--interactive` (the windowed
REPL, Stage 8's last sliver) and the Level 2 rect operators. Also
done, unplanned/post-roadmap: Stage 21, standalone installability —
`src/paths.rs` (the resource resolver used by the font catalog,
`run`/`file`, and the `handwrite`/`typewrite`/`photo-etch` subcommands (and `chalk`, #555); since #598 it also
serves a copy of `lib/**/*.ps` compiled into the binary, and a release
build has no checkout heuristics),
`scripts/package_release.sh`, and `.github/workflows/release.yml`,
so pscat can be installed and run without a git checkout (since #602
the tag-driven release pipeline: `scripts/release.sh X.Y.Z` opens the
release PR, a pushed `vX.Y.Z` tag builds five platform archives and the
crate, drafts the GitHub Release and publishes from the approval-gated
`release` environment; every publishing stage in
`scripts/release_stages.py` skips an identical artifact and halts on a
different one, so a failed release resumes by re-running; since #635 the
`publish` job signs build-provenance attestations, and since #638
`release-check.yml` installs every published release from the public
channels; procedure and owner setup in `docs/dev/RELEASING.md`) (since #599
the font catalog reaches installs as downloadable font packs on their
own `fonts-YYYY.MM` release train: `src/fontpacks/`, `pscat fonts
install`, `.github/workflows/release-fonts.yml`,
`docs/dev/RELEASING-FONTS.md`); and Stage
22, Korean/Japanese/Thai fonts — a `CatalogEncoding::Unicode` face
variant (Noto Sans KR/JP/Thai) whose `show` decodes UTF-8 and maps
codepoints straight to glyphs via `cmap`, since Hangul/kanji exceed
the 256-slot Encoding model everything else uses (see docs/dev/FONTS.md's
"Unicode-mode catalog faces" addendum and docs/archive/NOTES.md's Stage 22 entry);
and Stage 23, the Stage-22 follow-up issues (#3, #5) — gallery/site
entries for the four international faces, `.ttc` as a recognized
catalog extension, and a second Korean face (Nanum Brush Script,
brush-calligraphy style) on the same Unicode-mode mechanism (docs/archive/NOTES.md's
Stage 23 entry). Also done: issue #12, circular/curved-text procedures
in artkit (`ctext`/`ctextctr`), and issue #8, PDF document metadata —
`%%Title:`/`%%For:` DSC header comments now populate the exported
PDF's `/Info` dict, verified against gs's own pdfwrite behavior on the
same comments; also fixed a real pre-existing bug found while building
its demo example, where `stroke`'s PDF recording was silently
no-op'd whenever `--pdf` was used without `--svg` (docs/archive/NOTES.md's entry
has the full story). Also done: issue #20, axial/radial gradient
(shading) fill support — the `shfill` operator (`src/shading.rs`,
`src/ops/shading.rs`, `Gfx::shfill`), `ShadingType` 2/3 with
`FunctionType` 2/3, native SVG `<linearGradient>`/`<radialGradient>`
export, a PDF average-color-fill approximation, and `lib/artkit.ps`'s
"gradients" section (`gradfn`/`axialsh`/`radialsh`/`gradfill`) on top of it
(docs/archive/NOTES.md's entry has the full story). Also done: issue #9, a tiling/tessellation
library for artkit — `lattice`/`hex`/`tri`/`hexgrid`/`trigrid`/
`truchet`, `examples/tiling.ps`, and the gallery piece `Woven
Labyrinth` (docs/archive/NOTES.md's entry has the full story). Also done: issue #6,
a procedural jittered-stroke Hangul face (jamo composition) —
`lib/hangul.ps`'s `/HangulScript`, built on a new interpreter-side
extension (`/UnicodeBuildChar true`, docs/dev/FONTS.md's "Unicode-mode Type 3
BuildChar" addendum) that lets a Type 3 `BuildChar` receive a full
Unicode codepoint instead of a byte; Unicode's Hangul-syllable
arithmetic decomposes each codepoint into its jamo, which compose from
just 14 atomic consonant stroke shapes plus a handful of vowel
primitives (docs/archive/NOTES.md's entry has the full story). Also done: issue #10,
hyperbolic geometry in the Poincare disk — `lib/artkit.ps`'s
`hpoint`/`hpolar`/`horthocircle`/`hreflect`/`hgeo`/`hpoly`/`httile`
(a breadth-first-reflection generator for regular {p,q} tessellations),
`examples/hyperbolic.ps`, and the gallery piece `Infinite Descent`
(docs/archive/NOTES.md's entry has the full story, including two real bugs a
standalone Python prototype caught before either reached PostScript: a
backwards circumradius convention, and a dedup-tolerance formula that
silently capped tile growth with depth). Also done: issue #11, fractal
/ self-similar-geometry procedures for artkit — `edgefractal`/
`edgepoly` (generalized Koch-style edge replacement, any turn-delta
generator, presets `/koch`/`/quadkoch` in `FractalGens`) and `gasket`/
`carpet` (Sierpinski-style area subdivision, triangle and square),
`examples/fractals.ps`, and the gallery piece `Recursive Peaks`
(docs/archive/NOTES.md's entry has the full story, including why `gasket`/`carpet`
ended up iterative rather than recursive — a recursive version leaves
every ancestor's dict frame open exactly when a leaf invokes the
caller's proc, silently breaking a plain `def`-based counter). Also
done: issue #13, 2D/3D function-graphing procedures — a new sibling
library, `lib/graph.ps` (no dependency on artkit either way):
`setframe`/`gmapx`/`gmapy`/`gmoveto`/`glineto` map a data-space domain
onto a device-space viewport, `plotfn`/`plotparam`/`plotpolar` sample
curves into the current path, `axes` frames them; `setview`/
`project3` add an azimuth/elevation camera and `plotsurface`/
`surfacerow`/`surfacecol`/`axes3` a `z=f(x,y)` wireframe mesh (no
hidden-surface removal — a documented scope cut). `examples/graphing.ps`
is the specimen sheet; the gallery piece Ripple Range demonstrates the
one-piece-specific trick that gets occlusion anyway: back-to-front
filled rows (docs/archive/NOTES.md's entry has the full story). Also done: issue
#14, a data-visualization chart library — `lib/dataviz.ps` (also no
dependency on artkit or graph.ps): `setdvframe`/`dvmapy`/`dvcatx` map
a value domain and category count onto a device viewport for
`barchart`/`linechart`/`areachart`; `setscatterframe` (the same 8-arg
shape as graph.ps's `setframe`) backs `scatterchart`; `piechart` draws
pie wedges or, given a nonzero inner radius, donuts; `dvaxes` frames
the categorical charts. `examples/dataviz.ps` is a six-panel specimen
sheet; the gallery piece Field Notes pairs a bar chart and a line
chart on one shared category axis with a species-mix donut (docs/archive/NOTES.md's
entry has the full story, including two bug classes deliberately
tested for — a donut filled solid to the center, and a pie wedge
sweeping the wrong direction — both invisible to a plain ink-count
check). Also done: issue #15, a photo-to-line-etching utility — a
third sibling library, `lib/etching.ps` (no dependency on artkit,
graph.ps, or dataviz.ps): `et-dims` walks a JPEG's own marker segments
to read width/height/component count without decoding it; `et-draw`
opens the same file through `/DCTDecode` (a general filter here, so a
PS program can `readstring` decoded samples directly, bypassing the
`image` operator entirely — confirmed empirically before writing any
of this, which is what kept the whole feature to a PS library with
zero new Rust) and hatches it into a line engraving: parallel strokes
whose width tracks local darkness, plus a perpendicular crosshatch
pass in the shadows — quantized into a few width buckets with one
stroke per constant-bucket run, not one stroke per sample, since
stroke count (not sample count) is what actually costs render time.
`pscat photo-etch` (`scripts/photo_etch.sh` wraps it) does it end to end; `examples/etching_demo.ps`
is the specimen sheet (docs/archive/NOTES.md's entry has the full story, including
a real upside-down-photo bug the coverage tests caught: PostScript's
y runs bottom-up but decoded JPEG rows run top-down, so the first
version of `et-hatch`'s sample lookup rendered every photo flipped
vertically until that mapping got a `ph y sub`). Also done: issue #16,
paragraph/flowing-text layout for artkit — `lib/artkit.ps`'s
`tfwrap`/`tfdrawline`/`tfflow`/`tfblock`/`tfcols`: greedy word-wrap,
four alignments (including `/justify`'s stretch-to-margin, skipped on
a paragraph's last line), and columns, all built on `tfflow` taking a
`boundsproc` (`{y -> x0 x1}`, called per line) so a region's width can
vary with height instead of being locked to a rectangle — `tfblock`/
`tfcols` are just `tfflow` with a constant-width boundsproc.
`examples/paragraph_layout.ps` and the gallery piece `The Compositor's
Proof` (`gallery/compositors_proof.ps`, a motto flowed into a circular
medallion via a hand-written boundsproc) demonstrate it (docs/archive/NOTES.md's
entry has the full story, including two real bugs caught before
merge — one by `advisor` plan review, one by actually rendering the
example, that a pixel-adjacent-but-not-quite regression test had
missed). Also done: issue #17, a self-check/lint mode for
agent-driven rendering — `src/lint.rs` (`--lint` on the CLI, wired
into `pscat-mcp`'s `render_postscript`) flags a blank page, an
unbalanced `gsave`, and operand-/dict-stack leaks; `error_report`
gained a `Line: N` source-line attribution, scoped to the top-level
program only (see the "Deliberate deviations" list below for why)
(docs/archive/NOTES.md's entry has the full story, including two false-positive
traps `advisor` caught before any code existed, and two real,
previously undetected operand-stack leaks — in `lib/artkit.ps`'s
`tfdrawline` `/justify` and `lib/etching.ps`'s `et-hatch` — that
running `--lint` over `examples/`/`gallery/` found and this issue
fixed alongside the feature). Also done: issue #18, parameterized page
templates -- a fourth sibling library, `lib/pagekit.ps` (depends on
`artkit.ps`, unlike `graph.ps`/`dataviz.ps`/`etching.ps`): `pgcard`/
`pgletter`/`pgcertificate`/`pginvitation`/`pgposter`, each
`x y w h dict pgNAME` filling a region from a content dict of optional
keys and returning `tfblock`'s leftover-text contract.
`examples/template_*.ps` is one specimen per template (docs/archive/NOTES.md's
entry has the full story, including two real bugs `advisor`'s plan
review caught before any template code existed -- artkit's `fitfont`
enlarging as well as shrinking, and two of artkit's mood palettes not
actually being ordered dark-to-light despite looking like they were --
and one real implementation bug `--lint` caught on the first rendered
example, a copy-paste-misaligned `tfblock` call in `pgletter`). Also
done: issue #19, noise and flow-field procedures for artkit —
`noiseinit`/`noise2` (2D gradient/Perlin noise off a `srand`-shuffled
permutation table), `curl2` (turns any `{x y -> n}` scalar field into
a unit-vector flow via its normalized perpendicular gradient), and
`advect` (traces a particle through a `{x y -> dx dy}` field as
`lineto`s). All three use plain global scratch, no caller proc gets a
private dict opened for it during `exec` — an original draft did wrap
`curl2`/`advect` (they take a caller-supplied field proc, so the
gasket/carpet/hexgrid nested-composition gotcha applies), but a
cross-model (Codex) review at the PR stage found that auto-wrapping
silently swallows any ordinary (non-nesting) field proc's own plain
`def`-based state; switched to `gasket`/`carpet`'s own precedent
instead (library stays unwrapped, caller wraps their own nested call).
`examples/noise.ps` and the gallery piece `Lodestone`
(`gallery/lodestone.ps`, 1,400 `advect`-traced iron filings curling
around a jittered rock) demonstrate it (docs/archive/NOTES.md's entry has the full
story, including that Codex review — it also caught `curl2`'s
docstring overclaiming exact divergence-freedom for its normalized
output, measured at ~-0.27 for one test field, now documented
accurately — and two real bugs caught empirically before either
reached a permanent test: `and 255` vs. `mod` for negative lattice
coordinates, and a field proc that doesn't consume its `x y` silently
leaking stack values instead of erroring — the same class of bug
`--lint` also caught directly in `examples/noise.ps`'s first draft).
Also done: issue #21, sweep/contact-sheet rendering — `--sweep-seed`
(overrides every `srand` call transparently, except a restore of an
`rrand` value (#202), via a new
`Interp::set_seed_override`/`seed_override_fired` pair checked in
`ops/arith.rs`'s `srand`, so found art with a hardcoded `N srand` line
sweeps unmodified) and `--sweep NAME=` (predefines `/NAME` in userdict
via a second `run_source` call before the real one, for a source that
opts in to read it — no source-line-shift text-mangling); a new
`src/contact_sheet.rs` composites same-sized frames into one grid PNG,
capped at the same 8000px-per-side ceiling `--page` already enforces.
`examples/sweep_demo.ps` demonstrates both mechanisms (docs/archive/NOTES.md's
entry has the full story, including why a plain `pscat-mcp` tool
wasn't added -- a documented scope cut, not an oversight). Also done:
issue #39, a machine-readable catalog of agent-usable art
capabilities -- `--capabilities` on the CLI and `describe_art_
capabilities` on `pscat-mcp` both print one JSON payload covering
fonts, the Type 3 program faces, artkit's mood palettes, the
`pagekit.ps` templates, and artkit's/the style packs' major
procedures (`src/capabilities.rs`). Fonts are the one section built
dynamically, off the same `font::catalog_entries()` `--fonts` and
`findfont` resolution already use, so that section can't drift from
what's actually installed; everything else is hand-maintained (no
docstring convention in PostScript for this module to parse) but kept
honest by `tests/capabilities.rs`, which loads each `.ps` source into
a real `Interp` and checks the name set both ways -- every cataloged
name still exists, and every name a source file actually defines is
either cataloged or on an explicit internal-helper allowlist. See
docs/dev/CAPABILITIES.md for the payload shape and how to register a new
capability (docs/archive/NOTES.md's entry has the full story). Since #94,
files carrying `% @` tags are parsed by `build.rs` instead; see CAPABILITIES.md. Native pscat-extension operators are `kind: "operator"` rows (#754).
Also done: issue
#48, deterministic scatter and distribution primitives for artkit --
the area-shaped counterpart to `alongpath`: `screct`/`scpath` build a
region (a rectangle, or the current path flattened and closed the way
`fill` sees it -- `clippath scpath` for the clip region),
`scin`/`scarea` interrogate one, and `scatter` places a
caller-supplied mark across it by fixed count or by density, with a
`/Weight` procedure for non-uniform distributions, seeded
scale/rotation variation, exact `/MinSpacing` (a sparse hash grid in
an ordinary PostScript dict, cells of MinSpacing/1.5 so at most one
mark per cell, 5x5 neighborhood -- not an O(n^2) scan), and a hard
deposit budget. Containment is a real crossing test over the captured
edges, so candidates outside a shape are *rejected* rather than drawn
and clipped. Three scratch prefixes (`sc-`/`sq-`/`si-`) rather than
one, because the natural way to write a non-uniform scatter is a
`/Weight` proc that calls `scin` from inside scatter's own loop.
`examples/scatter.ps` and the gallery piece `Firefly Census`
(`gallery/firefly_census.ps`, a night meadow where every mark on the
page is scattered and none placed by hand) demonstrate it (docs/archive/NOTES.md's
entry has the full story, including why `/Seed` saves and restores the
caller's random stream with `rrand` instead of just calling `srand`,
and why gs agrees on scatter's counts but not its placements). Also
done: issue #49, a reusable hatching and cross-hatching library — a
sixth sibling, `lib/hatchkit.ps` (no dependency on `artkit.ps` or any
other sibling): one operator, `hatch`, fills whatever region is
currently *clipped* with parallel line strokes, leaning entirely on
the graphics state's own `clip` rather than reimplementing
point-in-polygon math — lines sweep past the region's real boundary
and get cut to shape by the ambient clip, so concave and
self-intersecting regions clip exactly as cleanly as convex ones.
Single or multi-angle (`/Angles`, one full layered pass per angle —
cross-hatching is just two calls 90 degrees apart), seeded
width/wobble/dropout/length variation, and an optional `/Density`
callback (clamped into `[0,1]` regardless of what it returns, and
bounded by two independent deterministic-from-`/BBox`/`/Spacing`
pre-flight checks — `/MaxLines` and `/MaxSamples` — computed and
enforced before any drawing or RNG draw, so a pathological `/Spacing`
or callback can change tone but never spawn unbounded geometry).
`examples/hatching.ps` is a three-panel specimen sheet (flat shading,
a `/Density`-driven tonal band that reads as a curved lit sphere from
perfectly straight strokes, and layered cross-hatching). Tag-migrated
into the doc-comment capability catalog (issue #94) from the start,
rather than added to `build.rs`'s `LEGACY_FILES` (docs/archive/NOTES.md's entry has
the full story, including two real implementation bugs a `--headless`
render actually caught rather than reasoning about the PostScript: an
`hkclipseg` initial range too narrow to ever draw a full-length line,
and `/Density`'s own value stored under a bare name that silently
auto-executed the caller's callback mid-setup — the exact footgun this
file's own default-wrapping helper exists to avoid, applied
inconsistently to a second name in the same file).
Also done: issue #50, density-driven stippling and point-shading — a
seventh sibling, `lib/stipplekit.ps` (`@requires: (lib/artkit.ps) run`,
tag-migrated from the start like hatchkit). One operator, `stipple`, a
thin convenience layer over `scatter` (issue #48) rather than a second
placement engine — the region operand and every option not about
density (`/Count`, `/MinSpacing`, `/Seed`, `/Tries`, `/Budget`,
`/Scale`, `/Rotate`) are `scatter`'s own, forwarded unchanged.
`/Density` is a plain number (forwarded verbatim as `scatter`'s own
`/Density`) or a `{x y -> w}` relative-tone callback paired with a
required `/MaxDensity` (peak marks per area) — internally `stipple`
hands `scatter` `/MaxDensity` as its own `/Density` (driving `Count`)
and the callback, unwrapped, as `scatter`'s own `/Weight` (driving
shape): two existing mechanisms recombined, no new placement
arithmetic. The realized total deliberately tracks that peak times the
region's area, not the field's own spatial integral — `scatter`'s
retry-until-accepted `/Tries` loop makes the two diverge, a design bug
the advisor caught before any code existed (docs/archive/NOTES.md's entry has the
full story, including a second, independent bug hit during manual
smoke testing: the exact same bare-name auto-execution footgun
hatchkit's own entry above describes, in a brand-new file that didn't
reuse hatchkit's code and so didn't inherit its fix). Default `/Mark`
is a filled circle sized by `scatter`'s own `/Scale` range (no new
size-variation vocabulary); a caller-supplied `/Mark` overrides it
entirely for real point-shading. `examples/stippling.ps` is a
three-panel specimen (constant density, a callback-driven sparse-to-
dense tonal ramp, point-shading with a custom rotated-cross mark); no
gallery/site entry, matching hatchkit's own precedent that a primitive
gets an `examples/` specimen, not a gallery card.
Also done: issue #53, reusable halftone screens and misregistration
offsets — an eighth sibling, `lib/halftonekit.ps` (no sibling
dependencies: a halftone is a deterministic lattice, not random
placement, so routing it through `scatter` would add noise the medium
is defined by not having). One operator, `halftone`, fills the current
clip with a dot, line, or cross-line screen (`/Screen`, compared with
the language's own `eq` — a `(dot)` string selects the same screen as
`/dot`, verified identical in Ghostscript): per-cell tone from a
number or `{x y -> w}` callback (clamped before `sqrt` ever sees it),
`/Frequency` in cells per inch, per-layer `/Offset` via an
unconditional `translate`, and a single deterministic `/MaxCells`
pre-flight budget (one tone call and a fixed 1-or-2 marks per cell, so
one count bounds callbacks, geometry, and ink together — hatchkit's
two-budget shape exists only because its per-line sample counts vary).
`examples/halftone.ps` is a four-panel specimen ending in a
misregistered two-plate spread; no gallery/site entry, same primitive
precedent (docs/archive/NOTES.md's entry has the full story, including two real
bugs smoke-rendering caught: a two-operand `exch` on a one-operand
operator, and a normal-vector/count name collision that parked the
whole lattice at (560, 540)).
Also done: issue #51, paper/canvas/print-surface textures — a ninth
sibling, `lib/surfacekit.ps` (`@requires: (lib/artkit.ps) run`,
tag-migrated from the start). Five presets: `grain`/`fiber`/`scuff`/
`misreg` are thin `scatter` wrappers (the same "no new placement
engine" choice `stipplekit.ps` made); `weave` is its own grid-and-`scin`
loop with two independent pre-flight budgets (`/MaxThreads` on cell
count, `/MaxEdgeSamples` on cell-count-times-edge-count for a `scpath`
path region — mirroring `hatchkit.ps`'s own `/MaxLines`/`/MaxSamples`
shape). New `/Color`/`/Strength` options, each preset's call wrapped in
its own `gsave`/`grestore` since every default mark sets color.
`examples/surfacekit.ps` is a six-panel specimen sheet (docs/archive/NOTES.md's
entry has the full story, including why the "scratches and scuffs"
preset is named `scuff` rather than `scratch` — this codebase already
uses "scratch" as a term of art for private working state throughout
every sibling library's own docs, and five review rounds' worth of
auto-execution and unguarded-numeric-conversion hazards a caller-
supplied value could trigger before its type was checked).
Also done: issue #146, apparel template — `lib/apparel.ps` (`apmain`,
config dict -> per-piece front/back/optional sleeve artwork), `examples/
apparel_shirt.ps` + `apparel_second.ps`, `scripts/apparel_export.sh` (finished shirt art, issue #147: `examples/keepin_it_real_shirt.ps`, `art/keepin-it-real/`, `docs/archive/keepin_it_real_shirt.md`)
(PNG/SVG/PDF + manifest + proof), and the new `--transparent` flag (PNG
alpha / SVG without backdrop). Details and the observed pre-fix
transparency behaviour in docs/archive/NOTES.md. Next in this thread (#155 landed: `/Chewy-Regular`, `/BagelFatOne-Regular`, see docs/archive/groovy_font_choice.md): #147 (the actual
shirt) is now just a config.
Also done: issue #167, jersey realism -- `lib/jersey.ps` gained
`/Outline2`, `/Shadow`, `/Tracking`, `/NameWidth`; `gallery/varsity_backs.ps`
draws two jersey backs carrying it (the garment is specimen art, not
library). See docs/archive/NOTES.md for what made it read as a jersey.
Also done: issue #149, arched jersey layout — `lib/jersey.ps`
(`@requires: (lib/artkit.ps) run`, tag-migrated): `jerseylayout` /
`jerseydraw` / `jersey` place an ink-centred, midpoint-tangent arched name
over a dominant number (+ optional caption) in a print rectangle, with
outline + fill, light/dark presets, tilt/dominance limits, and named
rejections (`jersey-name-too-wide`, ...). `jerseyarch` is the reusable
path builder; `ctext`/`ctextctr` are unchanged (docs/archive/NOTES.md says why they
weren't enough). `examples/jersey.ps`, `tests/jersey.rs`, a site card.
Also done: issue #145, headline composition helpers — `lib/headline.ps`
(`@requires: (lib/artkit.ps) run`, tag-migrated): `hllayout` places a
dominant run plus supporting runs by anchors/offsets against measured
*ink* boxes (whole runs or glyph ranges `[i,j)`), sized by `/Size`,
`/Fit*` or `/SizeOf`, and fails with named errors
(`headline-run-outside-region`, `headline-runs-collide`,
`headline-fit-below-minimum-size`, ...) instead of clipping; `hldraw`
draws (per-run `/Draw` proc, so `psyletter` can slot in),
`hlrunink`/`hlcharink`/`hlink` read the geometry back. Gotcha worth
remembering repo-wide: `pathbbox` after `charpath` includes the trailing
advance-point `moveto` (gs ignores a trailing moveto; #316) and
(unflattened) Bezier control points — walk the flattened path instead
(docs/archive/NOTES.md has the details). Under a rotated or skewed CTM `pathbbox` is
gs's box: the user-space box of the *device* bbox, larger than the tight
user-space box (#288, `notes/288-pathbbox-rotated-ctm.md`).
`examples/headline.ps` is the specimen (shirt front + `Tuesday`), with a
site card. The final shirt using #144's treatment is a follow-up.
Also done: issue #144, psychedelic display lettering — `lib/lettering.ps`
(`@requires: (lib/artkit.ps) run`, tag-migrated): `psyletter` draws an
outline face's `charpath` with a clipped multicolor interior
(`/Treatment` `/solid` `/transition` `/patches` `/mottled`) under a dark
contour; `psyletterpath` is the geometry alone. Default face
`/PermanentMarker`; a missing face raises `lettering-font-not-found`
rather than `findfont`'s silent Helvetica. `examples/lettering.ps` is
the specimen, with a site card (docs/archive/NOTES.md's entry has the vector-vs-raster
table and the gs caveats). First slice of the shirt remake; composition
and print assets are follow-up issues.
Also done: issue #154, GIMP-style fills — `lib/fillkit.ps`
(`@requires: (lib/artkit.ps) run`, tag-migrated): `plasmafill`,
`solidnoisefill`, `diffcloudsfill`, `gradshapefill` over `x0 y0 x1 y1 opts`,
each one `colorimage` under the caller's clip. `psyletter` gained
`/Treatment` `/plasma` `/clouds` `/diffclouds` `/conical` (need fillkit
loaded, else `lettering-treatment-needs-fillkit`). `examples/fillkit.ps`
is the specimen; docs/archive/NOTES.md's entry lists what GIMP algorithms were skipped.
Also done: issue #52, woodcut/linocut/engraving mark presets — a
tenth sibling, `lib/printkit.ps`, composing `hatchkit.ps`'s `hatch`,
`artkit.ps`'s `scatter`, and (optionally, `/Paper true`)
`surfacekit.ps`'s `grain` into three presets over one shared options
dict (`/Scale`/`/Density`/`/Roughness`/`/Seed`/`/Color`/`/Budget`/
`/Paper`/`/Angle`). Deliberately breaks from its siblings' `region opts
NAME` convention — each preset takes the *current path* directly (like
`hatch` itself) and manages its own `clip` internally, since
reconstructing an exact `clip`-able path back out of a stored region's
flattened edge soup would be real machinery for no benefit over the
path the caller already has. `examples/printkit.ps` is a four-panel
specimen sheet; the gallery piece *Nightfall, Three Cuts*
(`gallery/nightfall_triptych.ps`) uses all three presets in one
moonlit scene, a deliberate departure from the "primitive gets a
specimen, not a gallery card" precedent its own siblings set, since
issue #52 explicitly asked for a gallery composition (docs/archive/NOTES.md's
entry has the full story, including three design-review findings
`advisor` caught before any code existed — never trust `hatch`'s own
`pathbbox` default across a call that changes the current path first,
clip the true path before flattening one via `scpath`, and a
`/Budget` default that matches `hatch`'s/`scatter`'s own rather than
silently lowering either's ceiling).
Written for whichever model
picks the project up next — read this after `CLAUDE.md` and before
touching code. `docs/archive/ROADMAP.md` has the task list with model routing;
`docs/archive/NOTES.md` and `notes/` have the per-stage histories (`notes/` is newer); this file is the *orientation*.

## Where things stand

**623 tests across 40 suites, clippy clean.** Stages 1–19 are done,
including Stage 8's last sliver, the `--interactive` windowed REPL
(`-i`; stdin reader thread → `EventLoopProxy` user events → chunks
run on the frame budget; line accumulation shared with the terminal
REPL via `src/repl.rs`). Stage 8 delivered: **save/restore** as
object-granularity copy-on-write journaling (`docs/dev/VM.md` is the design
doc and gs-pin record — read it before touching any operator that
mutates array/dict contents; new mutators call
`Interp::journal_array`/`journal_dict` first, strings exempt by
spec); **name interning** (`src/name.rs`, fib 214→137ms);
**Indexed/Separation color spaces** (tint transforms run via the
generic `Frame::PostOp` continuation — copy that pattern for any
operator needing a procedure's result); **Level 2 odds and ends**
(`ops/level2.rs`); and **corpus round 2** (`tests/corpus.rs`): tiger,
golfer, escher, colorcir, doretree render block-identical to gs.
Stage 9 delivered the output targets: real multi-page `showpage`
(lazy erase — the window keeps the finished page), `--dpi`, and
`--svg`/`--pdf` export (paint-pipeline mirrors; the PDF is verified
by gs rasterizing it back). The interpreter runs found PostScript
with data structures, error recovery, text in three font technologies
(bundled TrueType via ttf-parser, Type 3 glyph procedures, Type 1
charstrings), file objects and decode filters, and sampled images.
Everything is golden-tested against Ghostscript (`tests/golden.rs`
renders eight examples in both and compares block-downsampled output).

## The three ideas that explain most of the code

1. **The machine is an explicit frame stack** (`src/interp.rs`,
   `Frame` enum). Anything that must interleave with PostScript
   execution is a frame: loops, `stopped` boundaries, the show family
   (`Frame::Show` → `font::ShowCtx`), images (`Frame::Image` →
   `image::ImageCtx`). Frame order **is** program order — that's why
   `stringwidth` can push its result at frame-pop, why Type 3
   BuildChar and kshow procs nest arbitrarily, and why the live window
   renders glyph-by-glyph. If a new feature needs to run a PostScript
   procedure mid-operator (patterns, `cshow`, resource callbacks),
   copy the ShowCtx pattern; do NOT call the machine recursively.

2. **Files share one read cursor** (`src/file.rs`). The scanner pulls
   bytes through a `FileHandle`; `currentfile` returns the same
   handle; filters are files layered on files, decoding *on demand* so
   a filter consumes exactly the source bytes its consumer needed.
   This single property is what makes `currentfile eexec` (Type 1),
   `N RD <binary>` (charstrings), `currentfile buf readhexstring`
   prologs, and inline image data all work. Two conventions were
   pinned against gs and must not regress:
   - a token terminated by whitespace consumes that one delimiter
     character (`lexer::eat_token_delimiter`);
   - `image` drains a filter-chain source layer-by-layer to each EOD
     marker on completion.
   An image's footprint is ImageMatrix⁻¹ of its source rectangle, not
   the unit square; only the `[w 0 0 ±h …]` idiom keeps the old
   unit-square clip, for byte-identical edges (#534,
   `notes/534-image-matrix-footprint.md`).

3. **Every glyph source feeds one transform pipeline.** Glyph-space
   outline → FontMatrix (composed by scalefont/makefont, cached at
   setfont) → CTM at the pen → device `PsPath` → the ordinary
   fill/clip machinery. TTF outlines (`font::outline_glyph`), Type 1
   charstrings (`font::type1_glyph` + `src/type1.rs`), and Type 3
   procedures (CTM set so BuildChar draws in glyph space) all end in
   the same place. A new glyph source (Type 42-from-program, CFF)
   should too.

## Deliberate deviations (all documented in place)

- `FID` is integertype; unknown `findfont` substitutes Helvetica
  (gs-style) instead of erroring. Symbol/ZapfDingbats have real URW
  faces since Stage 18 — via the runtime catalog (`fonts/catalog/`,
  `font::resolve`). Since #599 pscat knows which pack a face is in; a
  face from a pack that is not installed (Symbol and ZapfDingbats
  included) was `invalidfont` with a hint naming the pack. Since #728 it
  stands in as the closest built-in face (`font::stand_in_for`: serifs to
  Times, AvantGarde to Helvetica, ZapfChancery to Times-Italic, Symbol and
  ZapfDingbats to Helvetica) with one stderr note per name naming the pack
  (`Interp::font_notes_shown`), and the run exits 0. `lettering`,
  `headline` and `jersey` still reject a stand-in with their own
  `*-font-not-found`. wasm, which has no packs, substitutes Helvetica
  silently. `tests/skill_blocks.rs` runs the plugin skills' `postscript`
  blocks on a bare install (`PSCAT_NO_CHECKOUT=1`, a debug-build switch that
  hides the checkout's `lib/` and `fonts/catalog/`) and fails on a
  font-pack note, so a skill example cannot need a pack; a fragment is
  fenced `postscript partial`.
- `charpath` on Type 3 advances without capturing outlines; its bool
  operand is ignored for outline fonts.
- Type 1 hints are ignored entirely; no `/Metrics`, no CID/CFF.
- Access control (`executeonly` etc.) is not modeled — identities.
- Filter parameter dicts accepted and ignored; no encode filters.
- Images: nearest-neighbor unless the dict has `/Interpolate true` (#730:
  bilinear on a magnified axis, sample centres at i+0.5, edges clamped,
  colour images only; PDF key and SVG `image-rendering="auto"` follow;
  `notes/730-image-interpolate.md`). Samples are 1, 2, 4, 8, 12 or 16
  bits, `colorimage` `multi true` and `/MultipleDataSources true` read one
  source per component (#781, `notes/781-image-wide-samples-multi-source.md`);
  wide samples are reduced to 8 bits in PNG, SVG and PDF alike. `ImageType 3` has
  `InterleaveType 3` only (types 1 and 2 are limitcheck) and no
  `ImageType 4` (#739, `notes/739-imagetype3-nulldevice.md`). DCTDecode buffers one
  whole JPEG (marker-aware, stops exactly at EOI); CCITTFax absent;
  Adobe inverted-CMYK JPEGs untested.
  `ImageCtx::rgb_at` is the one place a sample becomes a colour; for 8 bits
  or fewer it reads a per-component decode table built from the same
  expression (`decode_value`), so a speed-up there must stay bit-identical
  (#793, #835; `notes/835-image-rgb-at-decode-table.md`).
  `ImageCtx::fill` reads a file source with one `PsFile::read_into` per call
  (#896, `notes/896-image-bulk-read.md`); the decoders under a filter still
  pull their own input one byte at a time, so they never read past EOD.
- errordict holds a handler per error name (#372, so `errordict /name
  get exec` re-raises), but a handler a program installs there is not
  consulted on a real error; error-time operand-stack restoration not
  done (PLRM handlers see pre-error operands).
- `nulldevice` is a flag in the graphics state (#739): marking operators
  consume their operands and paint and record nothing, `showpage`/
  `copypage` emit no page, the CTM and `defaultmatrix` are identity.
  There is no `setdevice`/`currentdevice`, so nothing else can install a
  device.
- Stroke width ≈ √|det CTM| (anisotropic pens wrong; `strokepath`
  inherits it, since it shares `stroke`'s parameters); `showpage`
  doesn't erase; ints are i64; `flattenpath` uses a fixed
  quarter-pixel tolerance (`setflat` not modeled — chord counts
  differ from gs, shapes agree).
- `clippath` under a *nested* clip (issue #120, fixed): exact and
  DPI-invariant when every clip in the chain is a plain axis-aligned
  rectangle (`rectclip`'s shape — the common case), otherwise a pixel
  trace of the raster clip mask (`Gfx::mask_boundary_path`) rather
  than an analytically exact intersection — a superset by construction
  (any nonzero-coverage pixel counts as inside), not a pixel-perfect
  boundary. A single, non-nested clip is unaffected: it still returns
  the exact original path, bit-for-bit. It also drops the `eoclip` rule.
  For an exact capture and replay of the clip in force, use a Level 2
  `gstate` object (#394): `setgstate` restores the same mask and chain.
- `gstate` objects (#394): `gstate`, `currentgstate`, `setgstate`, and
  `copy` between two gstates; `currentgstate`/`copy` are journaled, so
  `restore` undoes them, as in gs. No local/global VM distinction, no
  `invalidaccess` for a read-only gstate. A program that `setgstate`s an
  older clip back makes the exporters reopen that clip's `Rc`; SVG reuses
  its `clipPath` id.
- `clipsave`/`cliprestore` (Level 3, #494), pinned against gs 10.07.1:
  the clip stack is part of the graphics state (gstate objects carry it),
  each `gsave` and each `save` starts a new, empty level, and a
  `cliprestore` with no entry at its level gives back the clip the level
  started with: the clip at that `gsave`/`save`, or no clip at the top
  level. So an unpaired `cliprestore` is not a no-op when a clip is in
  force. `initclip` and `initgraphics` keep the stack; `grestoreall`
  empties it. Only the clip
  changes. A Type 3 `BuildChar` opens its own level, as gs runs it inside
  a `gsave` (#500, `Gfx::glyph_enter`): an empty-stack `cliprestore` or an
  unmatched `grestore` there gives back the state at `show`
  (`notes/494-clipsave-cliprestore.md`, `notes/500-cliprestore-in-buildchar.md`).
- `grestore` with nothing to pop matches gs (#505): at a `save` boundary
  it brings back the save's state without popping (so it repeats); at the
  bottom it, and `grestoreall` with no `save`, bring back the job's
  initial state (`Gfx::initial_state`, no font). Both start an empty
  `clipsave` level. pscat's own paired calls (`rectfill`, ...) use the
  plain popping `Gfx::grestore`; only the operator goes through
  `Interp::do_grestore`. `currentgray`/`currenthsbcolor` report the
  `f32` colour's shortest decimal (`0.5`, not `0.49999999999999994`), and
  so does `currentrgbcolor` since #515 (`0.3 0.6 0.9`, not the widened
  `f32`s). Rounding it moved the alpine_lake still by 1 LSB, because
  paintkit does arithmetic on it, so that still was regenerated
  (`notes/515-currentrgbcolor-readback.md`). Reals in general still print
  shortest-`f64`, where gs prints 9 digits
  (`notes/505-grestore-bottom-currentgray.md`).
- Readbacks added in #513: `currentcmykcolor`, `currentcolor`,
  `currentlinecap`, `currentlinejoin`, `currentmiterlimit` and
  `currentstrokeadjust`, plus a `setstrokeadjust` that only stores its flag
  (`initgraphics` keeps it, as gs does). The state holds the CMYK
  components a colour was set with (`GraphicsState::cmyk`, cleared by
  `set_rgb`) and the Indexed index / Separation tint (`spot`), because the
  painting `rgb` cannot give them back. `setcolorspace` still does not
  reset the painting colour as gs does (`notes/513-readback-operators.md`).
- `shfill` (issue #20): `FunctionType` restricted to 2 (exponential)
  and 3 (stitching) — 0 (sampled) and 4 (calculator) unsupported,
  since those would need the same `Frame::PostOp` reentrancy
  `Separation`'s tint transform uses, which axial/radial gradients
  don't otherwise need. `ColorSpace` restricted to `/DeviceGray`/
  `/DeviceRGB`/`/DeviceCMYK` (no Indexed/Separation, no
  array-of-functions form). `/Range` is honored only on the top-level
  Function a shading dict names directly, not recursively per
  stitching leg. `/Extend` is validated but always behaves as
  `[true true]` — the `gsave <path> clip shfill grestore` idiom
  already bounds the painted region, so `false`'s "transparent beyond
  the axis" is an edge case that idiom never hits. SVG export gets
  real `<linearGradient>`/`<radialGradient>`; PDF export approximates
  a shading as a flat fill in the ramp's *position-weighted* average
  color (no pattern-colorspace machinery). SVG's two-circle radial
  model only renders faithfully when the focal circle sits entirely
  inside the outer one (`distance(centers) + focal_r <= outer_r`) — an
  off-center `ShadingType` 3 that fails that (valid PostScript; no
  such constraint exists there) isn't detected or worked around, so
  its SVG export can visibly diverge from the raster.
- `setalpha`/`currentalpha` and `setblendmode`/`currentblendmode`
  (issue #47) are **pscat extensions, not PLRM operators** — real
  Ghostscript has no PostScript-callable alpha operator at all
  (`.setfillconstantalpha`, `.setopacityalpha`,
  `.setstrokeconstantalpha`, `setalpha` are all `where`-undefined on
  gs 10.x; gs's transparency lives inside its PDF interpreter). Two
  consequences worth knowing before touching either:
    * **Alpha reaches `image`/`imagemask`/`colorimage` since #766.**
      `src/image.rs` blits samples straight into the pixmap rather than
      through `Gfx::paint()`, so it composites itself: `blend_pixel`
      (premultiplied, tiny-skia's Normal and Multiply formulas, pinned
      by a unit test against a real tiny-skia fill). The default (alpha 1,
      Normal) keeps the old arithmetic verbatim, so existing renders are
      byte-identical. SVG gives the `<image>` `opacity` and the blend on
      the outermost clip group; PDF selects the `ExtGState` before `Do`.
      `selftest/drivers/image_alpha.ps` covers it.
    * **A program using them will not render the same under plain `gs
      file.ps`.** The verification route for alpha-bearing content is
      `--pdf` plus gs rasterizing that PDF back (`tests/pdf.rs`'s
      `alpha_survives_the_round_trip_through_gs`), which is stronger
      than the `ghostscript_accepts_*` pattern, not weaker — it
      compares pixels rather than exit status. `lib/paintkit.ps`'s
      watercolor section carries a flatten-against-white fallback for
      the `gs file.ps` path; see `docs/dev/design/WATERCOLOR.md`.
  Blend modes are deliberately just `Normal`/`Multiply`, on pscat's own
  two-variant enum rather than a re-export of `tiny_skia::BlendMode`:
  each variant has to map to tiny-skia, SVG's `mix-blend-mode` *and*
  PDF's `ExtGState /BM`, and an exhaustive match on a local enum is
  what makes adding a third mode fail to compile until all three
  exporters learn about it.
- `gstate .sameclip bool` (issue #488) is a third pscat extension, dot-named
  because it is non-standard (gs has no `.sameclip`): true when the clip in
  force is the very clip chain the gstate holds (both none, or the same
  `Rc<ClipNode>`; `GraphicsState::same_clip`). Identity, not geometry: a
  re-clip with an identical path is false. Library code probes it with
  `systemdict /.sameclip known` (drykit's `dkPsameok`) and keeps the
  standard behaviour without it (`notes/488-sameclip.md`).
- The noise operators (issue #734, `src/ops/noise.rs`: `noiseseed`,
  `gnoise2`/`gnoise3`, `vnoise2`/`vnoise3`, `gnoise2d`, `fbm2`/`fbm3`,
  `vfbm2`/`vfbm3`, `ridged2`/`ridged3`) are pscat extensions with a
  frozen algorithm: f64 add/sub/mul in a fixed order, `floor` and table
  lookups only, so every platform and WASM give the same bits.
  `lib/noiseref.ps` is the PostScript reference, and `tests/noise.rs`
  holds every operator bit-equal to it and pins a few exact values.
  **Changing any constant or operation order changes every seeded render
  that uses them.** The table lives in `Interp::noise_perm`, apart from
  `rand_state`. `Interp::set_seed_override` (`--sweep-seed`, #753) also builds
  it from the sweep seed (`noise_from_override`); a program's `noiseseed`
  clears that and wins; sampling it counts as the override firing. `x y perm .permnoise2` is artkit's `noise2` formula
  computed with PostScript's int/real promotion, so artkit's `noise2`
  (now `{ Perm .permnoise2 }` when the operator exists) stays
  bit-identical, type included (`notes/734-native-noise.md`).
  `gallery/basin_and_range.ps` (#755) is the visible example: a shaded-relief
  map from `fbm2`/`ridged2` through a `colorimage` procedure that reads
  `/BandBBox` in device space (so it also holds under the 2x still), and is
  the picture where a change to the noise constants shows up (`notes/755-basin-and-range-noise-example.md`).
- `error_report`'s `Line: N` (issue #17) is best-effort, not exact
  source attribution: no `Object` carries a source position, so it
  reports the line of the most recent token scanned directly from the
  *top-level* program (`Lexer`'s `is_main` flag) — never a
  `run`-loaded library file, an eexec stream, or an executable
  string, which would misattribute to the wrong source entirely — and
  stays sticky across procedure calls, so an error deep inside a
  previously defined procedure gets the call site's line, not the
  procedure's definition site.

## Working conventions that matter (beyond AGENTS.md)

- **Test scratch paths: `common::scratch_dir(tag)` only** (#487). A fixed
  temp name makes two parallel `cargo test` runs clobber each other;
  `tests/scratch_paths.rs` enforces it.

- **`SECURITY.md` states what untrusted PostScript and `pscat-mcp` can do
  today, and was checked against the code** (#603). A change that adds or
  removes a file, process, network or MCP capability, or a resource limit,
  must update it in the same PR; #568 added the MCP bounds and #604 the
  interpreter limits (below), #740 and #763 the write and read roots (#772
  brought the file current for 0.7.0). `docs/STABILITY.md` says which surfaces need a deprecation
  period. `.github/CODEOWNERS` paths are owner-merged (`SDLC.md`, "Who merges").

- **Visible changes merge without the owner mid-cycle** (#674). A PR that
  changes how something renders is checked by an independent perceptual
  reviewer agent against the issue's perceptual acceptance check and
  `docs/dev/TASTE.md`, merged by the agent on a pass, and logged in
  `docs/dev/review-log/pending/` for the owner's read before the release
  (`SDLC.md`, "Who merges"). Agents propose follow-ups instead of filing
  them; running many issues at once is `docs/dev/DISPATCH.md`.

- **gs is the oracle.** Every semantics question in Stages 5–7 was
  settled by running the snippet in gs first and pinning the answer in
  a test comment. Keep doing that; it caught real design errors twice
  (glyph caching by encoded name; the token-delimiter rule).
- The `.notdef`/substitution philosophy: found files should *render*,
  not error, wherever the PLRM allows latitude — but record every
  such choice in the module docs and docs/archive/NOTES.md.
- Commit per coherent chunk with stage-prefixed messages; update
  the matching `docs/guides/` page and your notes fragment in the same or a closing commit.
- **Notes go in `notes/<issue>-<slug>.md`, one file per issue, not in
  `docs/archive/NOTES.md`** (#273): concurrent PRs each prepending to `docs/archive/NOTES.md`
  conflicted constantly. `docs/archive/NOTES.md` is frozen history up to 2026-10-13;
  `notes/README.md` has the format and how to list fragments newest first.

## Next work, in recommended order

1. **More gallery pieces** — the slot stays open by design. A new piece
   adds an entry to `gallery/manifest.json` (#251: the one list behind
   the site gallery, the README highlights and `show.sh`; never edit
   `gallery.html`) and runs `python3 scripts/gallery.py sync`.
   The entry's source gets its "View source" window on the site for free
   (#461: staged to `_site/source/`, shown by `site/source-viewer.js`).
   `gallery/README.md` is the brief, and Hundred Lines
   (`gallery/hundred_lines.ps`) shows the /HandScript font reused.
   The Stage 15 faces (`lib/fonts/`) and the Stage 20 style packs
   (`lib/styles/` — steampunk, psychedelic, scifi, toon, with
   specimen posters in `examples/style_*.ps`) are ready-made
   material — a neon nocturne, a star-chart page, a carved
   inscription, an orrery of brass.
2. Leftovers when they itch: the remaining fib/fern machine-loop gap
   vs gs (Stage 11 findings in docs/archive/NOTES.md — the untaken levers are
   representation changes), consulting a user-installed errordict
   handler, error-time operand
   restoration, DSC-comment tolerance and more corpus files,
   CCITTFax. (`rectfill`/`rectstroke`/`rectclip` landed 2026-07-18,
   found missing by `--interactive`'s first session.)
3. Worth knowing before touching export: SVG (`src/svg.rs`) and PDF
   (`src/pdf.rs`) both mirror the paint pipeline at the same seams
   in `Gfx` (fill, stroke, glyph fill, erase, prepare_paint,
   end_page, plus the image hook in `image.rs`). A third export
   target should promote those seams into a neutral display list
   first — the design note in `src/pdf.rs` says why that wasn't
   done at two.
   An exporter table keyed by clip-node identity must hold the `Rc`
   (SVG's `clip_ids` stores it, PDF's `open_clip` is one), never a bare
   `Rc::as_ptr` address: a freed node's address is recycled and aliases a
   different clip (#258/#261).
   PDF shares one `q <clip>` group across consecutive elements with
   the same clip `Rc` (#219), so a new PDF hook must go through
   `enter_clip` and keep its own inner `q ... Q`. Page content streams
   are Flate-compressed (#244), so tests reading PDF operators use
   `tests/common/mod.rs::pdf_text`.
4. Dry media (epic #232): **read `docs/dev/design/DRYMEDIA.md` and
   `docs/dev/design/drymedia/REFERENCES.md` before starting #234–#242.** The design
   (#233) is approved, with every recommended answer to its Q1–Q9.
   drykit reads paintkit's `@internal` `pj-` surface (`pjheight`,
   `pjhash`, `pjCTM`, `pjP`, `pjW`, `pjM`, `pjR`, `pjmemo`) without editing
   paintkit.ps. A change to those names breaks drykit: the
   `drykit-paintkit-contract` self-test in `lib/drykit.ps` pins them (#270
   moves them to `lib/toothkit.ps`). #234 built `lib/drykit.ps`:
   `dksurface`, `dkstroke`, `dkbatch` and `dkclip` (#312) are public, and `dkcatch` and
   `dkrand` are internal. The medium files (#235 chalk, #239 crayon, #241
   charcoal) are presets over `dkstroke`. Measured cost is in
   `notes/234-drykit-core.md` and DRYMEDIA.md 6.1. A full board plus ghost
   text is 25.2s in pscat against a 30s budget, after #307's `bind`
   (`notes/283-drykit-perf-remeasure.md`). #324 made `dkbudget` a soft limit:
   a chunk whose catch overflows is split and drawn again, and only a stroke
   with a single stop too wide for any catch raises, at record time, before
   anything is drawn (`notes/324-drykit-headroom.md`). `dkbatch` keeps each
   stroke's CTM, so hatch layers can be drawn rotated inside it. It allows
   one pigment per batch (an `/After` stroke may differ, #393).
   #241 shipped `lib/drycharcoal.ps` (`dhstroke`, `dhhatch`, `dhsmudge`,
   `dhlift`, `dhsketch`; `notes/241-drycharcoal.md`). `dhhatch` lays lines
   through hatchkit's `/Mark` hook and inks every layer in one `dkbatch`
   inside its own region clip. Inside a caller's batch it joins that batch
   through drykit's `dkclip` (#312, `notes/312-dkbatch-per-record-clip.md`):
   a `dkstroke` made inside `{ ... } dkclip` keeps that region when its
   batch renders later. Since #444 a plain `clip`/`eoclip`/`rectclip`
   inside a batch is kept too: `dkbatch` shadows the clip operators and
   `gsave`/`grestore` in userdict while its procedure runs, and the first
   `dkstroke` after a clip change takes one Level 2 `gstate` (#394), shared
   by the strokes after it, so they still group
   (`notes/444-plain-clip-in-a-batch.md`). Gotcha: a procedure bound before
   the batch calls the operators directly, so neither its clips nor its
   restores are seen; a callback that clips and strokes inside a bound
   `gsave exec grestore` wrapper leaves later strokes under that clip
   under Ghostscript (#459, a documented limit: standard PostScript
   cannot see that restore. Use `dkclip` in the callback, wrap the call in
   a `gsave ... grestore` the batch sees, or bind the wrapper inside the
   batch; `notes/459-bound-wrapper-grestore.md`). pscat is exact there:
   `dkstroke` asks the pscat-only `gstate .sameclip bool` whether its
   capture's clip is still in force and recaptures if not; drykit probes
   it at load (`dkPsameok`), so gs keeps the limit (#488,
   `notes/488-sameclip.md`).
   Where the interpreter has `cliprestore` (gs, and pscat since #494)
   `dkbatch` shadows it too and marks the batch dirty (#454,
   `notes/454-cliprestore-in-a-batch.md`).
   #393: `dkstroke /After true` in a batch is drawn on its own after the
   batch renders, in call order, in its own colour; `dhsmudge` and `dhlift`
   use it, so they run inside a batch, over everything the batch lays
   (`notes/393-smudge-lift-in-a-batch.md`).
   Hatching costs about 0.07 ms per square point per layer in pscat. In gs,
   drykit work after an artkit `scatter` (fallout) runs about 2x slower (#311).
   #313: `/drawing` paper's `/Mottle` is 1.5, so charcoal hatching breaks up in
   specks, not patches, at the default `/Relief 0.65` (`notes/313-drawing-mottle.md`).
   Slice G1 (#236) shipped `lib/strokeglyphs.ps`: skeleton glyphs as paths
   (`sgpath`, `sgwidth`, `sggrow`, `sgmetrics`). In its glyph data a point
   written twice is a sharp corner, and the midpoint rule never passes
   through interior points, so check new glyphs on a `/Jitter 0` render.
   `notes/236-strokeglyphs.md` lists where it departs from the design.
   Since #350 `sgpath` and `sgwidth` are strict by default: a character or
   glyph name with no glyph raises `sg-missing-glyph:<char>` before any
   ink, instead of advancing blank (the superscript two in #238's
   `(i² = -1)` used to vanish silently). `/Strict false` opts out.
   #339 filled the commonest gaps (relations, ∘ ∗ ⋆, ⟨ ⟩ ‖, ℏ ℓ, ι υ Υ,
   ∬ ∭ ∮, superscript digits derived from the digit skeletons):
   `notes/339-board-math-glyphs.md`.
   #366 added subscript digits and letters (the same derivation, lowered
   80 units) and the variant Greek letters, and mapped mathkit's `\varphi`
   and friends to them: `notes/366-subscripts-variant-greek.md`.
   #237 shipped `lib/mathkit.ps` (`mk-`): TeX-subset math laid out with
   TeX's box model as those skeletons (`mkpath`, `mkmetrics`, `mkalign`,
   and the annotations `mkframe`, `mkunderline`, `mkarrow`), with
   board-hand irregularity under `/Irregular`. TeX backslashes are doubled
   in PostScript strings; a single one is refused by name, never drawn
   as letters. Its layout recursion keeps locals in one small dict per
   call, so its public procedures unwind the dict stack on error
   (`mkentry`). `notes/237-mathkit.md` has the design calls.
   #340 added `\overbrace`/`\underbrace` (strokeglyphs' grown left brace,
   drawn under a CTM turned a quarter, no new grower), `\overset`
   `\underset` `\stackrel` `\substack`, the style switches, `\not`,
   `\bmod` `\pmod`, and TeX align's columns in `mkalign` (several `&` a
   line, `/ColumnGap`): `notes/340-mathkit-structures.md`.
   #439 added `\mod` `\pod`, `\nolimits` on a brace, mkalign's
   `/ColumnWander` (off by default, so older layouts are unchanged) and
   `/ColumnGap 0` as `alignat`, and the `mkbrace` annotation (the same
   turned brace between two points): `notes/439-mathkit-columns-braces.md`.
   Chalk (#235) shipped `lib/drychalk.ps`. `dcchalk` is the chalk preset
   over `dkstroke`. `dcboard` dresses a slate or green board: haze,
   `/Ghost` writing, swipes, wipes, ledge dust and a tray. It was tuned
   against the reference photos (`notes/235-drychalk.md` has the numbers,
   `docs/dev/design/drymedia/chalk_compare_235.jpg` the side-by-side).
   Ghostscript gotcha: **don't leave scratch names in userdict.** With
   artkit, paintkit and drykit loaded, userdict is about 80% full, and gs's
   open-hash lookups slow every operator once it fills further. drychalk's
   public procedures run inside a private dict (`dcscratch begin ... end`)
   (#307). drykit's `/Fallout` is slow in gs (#308), so drychalk draws its
   own dust.
   #238 is the chalk gallery piece, `gallery/chalkboard_lecture.ps`: Euler's
   formula on three `dcboard` panels. Draw each panel's chalk right after
   its own `dcboard`, because `dcchalk` takes the last board as its
   surface. A `/Ghost` proc that passes a procedure must `load` it. Never
   `def` an operator's name (`loop`, `lt`) in a piece: handscript and
   strokeglyphs aren't bound and will pick it up
   (`notes/238-chalkboard-lecture.md`). #532 gave it a final pass with
   mathkit's structures (underbraces, a starred `\overset`, `\pmod`,
   wandering columns, `mkbrace`): to place an `mkbrace` inside an `mkalign`
   line, measure the prefix with `mkmetrics` under that line's own seed
   (`/Seed` + 101 i) and add it to the record's `/Origin`
   (`notes/532-chalkboard-final-pass.md`).
   #555 is the front door: `pscat chalk 'LaTeX' -o board.png` (single
   backslashes, `\\` rows, a JSON config). The driver generator is
   `pscat::chalk` (`src/chalk.rs`, pure Rust, wasm-safe); the CLI
   (`src/subcmd/chalk.rs`) only writes what `chalk::render` returns, so the
   MCP tool (`chalkboard_math`, #688, shipped) and a web demo call the
   library, not re-derive the driver. It measures with `mkalign` (ink thrown away) before loading
   drychalk, so a TeX error costs no board, and maps mathkit's
   `mk-<kind>:<token>` to `ChalkError::Latex`. `tests/chalk.rs` pins five
   boards by pixel hash: any mathkit/strokeglyphs/drychalk/drykit change
   that moves a pixel moves them (re-pin after looking)
   (`notes/555-chalk-latex-cli.md`). #700: `chalk::lines` removes every math
   delimiter pair (several side by side, nested), and a stray, unclosed or
   mismatched one, or words outside the pairs, is a one-line `ChalkError::Latex`
   (`notes/700-chalk-separate-inline-pairs.md`). #699: a `gather` wrapper
   (`gather`, `gather*`, `gathered`; the innermost stripped row environment
   decides) centres each row, via mathkit's `mkalign /LineAlign /center`
   (default `/relation`, so `align` and every other caller is unchanged); one
   row, or any other wrapper, never gets the option
   (`notes/699-chalk-gather-centred.md`).
   #713: on the no-board overlay (touch 0.34, so grain shows) the radical's
   long bar and small strokes broke into gaps; drychalk's `/Firm` (default 0:
   byte-identical, `identity_check.sh --loader lib/drychalk.ps`) holds the
   taper and lift to the last ten widths, fades the tired middle on long
   strokes and presses short ones harder. `chalk::driver` passes `/Firm 1`
   only with no board, so the board pins never move; the overlay has its own
   (`notes/713-chalk-firm-overlay.md`).
   #239 shipped `lib/drycrayon.ps` (`dy-`): `dycrayon` (a core and a rim
   `dkstroke` per subpath, one wax level each), `dypaper`, the crayon box
   (`dybox`, `dycolors`, `dycolor`) and six child motions with paintkit's
   #184 contract. dycrayon joins a `dkbatch` the caller opened by reading
   `userdict /dkBatch` (pinned by `drycrayon-drykit-contract`). Passes and
   layering are declared (`/Pass`, `/Over`), because drykit can't read the
   page. `notes/239-drycrayon.md` has the tuning numbers against the crops.
   #240 is the gallery piece on it, `gallery/crayon_drawing.ps` (a child's
   house, family, sun and name); `notes/240-crayon-drawing.md`.
   Typewriter (epic #209): **read `docs/dev/design/TYPEWRITER.md` and
   `docs/dev/design/typewriter/REFERENCES.md` before starting #211–#215.** The
   #210 design (Courier Prime outlines embedded in a Type 3 `/Typewriter`,
   keyed-hash variation, clip-based ribbon ink) is approved. Its gotchas:
   `BuildChar` must use `setcharwidth` (gs replays a `setcachedevice`
   glyph) and stay pure (`stringwidth` runs it), and pscat's `def` rejects
   integer keys (#338; use `put`).
   #211 shipped `lib/typewriter.ps` (`tw-setup`, `tw-set`; state in the
   `TWState` side dict, reached through the font's `/TWS`) over the
   generated `lib/typewriter_outlines.ps` (`scripts/gen_typewriter_outlines.py`
   from `fonts/catalog/CourierPrime`). The ink is flat: fill plus a spread
   stroke of pressure x key weight x `/Ribbon`. The machine table already
   holds each key's clog fraction and one-side angle and strength for #212
   to draw (`tMbuild`). build.rs's catalog scan rejects `/x N dict def` and
   sees every top-level `def`, so the side dict and the font are filled
   inside `{ ... } exec` (`notes/211-typewriter-font.md`).
   #212 shipped the ribbon ink (`tInkStrike` in the "ribbon ink" section
   of `lib/typewriter.ps`): a weave clip (`eoclip` of hashed patches of
   jittered holes, built once at load), a feather pass, the body, clogged
   counters and heavy sides; `/InkModel false` is #211's flat ink, byte for
   byte. At `/StrikeWear 0` the weave is keyed by the key, so quirks-only
   rows still repeat; tests compare such strikes up to a few pixels of
   rounding (`nearly_same` in `tests/typewriter.rs`). Tune against
   `docs/dev/design/typewriter/ink_vs_real.png` (`docs/dev/design/typewriter/tools/ink_compare.py`);
   `notes/212-ribbon-ink.md`.
   #213 shipped the layout (`tw-type`, `tw-linecount`, `tw-pageheight`; the
   "layout" block at the end of `lib/typewriter.ps`, private names `ly*`),
   `pscat typewrite` (`--ps`, `--height fit`, `--reflow`; since #600 `scripts/typewrite.sh` is a thin wrapper that honours `PSCAT_BIN`),
   `-o` picks PNG/SVG/PDF by extension like `photo-etch` and refuses any other name (#614; `output_format` in `src/subcmd/mod.rs`),
   the `Typewriter` catalog row (hand-written: build.rs rejects Type3Face)
   and `examples/typewriter_memo.txt`. One layout makes the line list both
   `tw-linecount` and `tw-type` use. `notes/213-typewriter-layout.md`.
   #214 shipped `lib/released.ps` (`rel-page` wraps any content in aged paper,
   scan skew and lamp, photocopy generations, marks; `rel-paper`, `rel-stamp`,
   `rel-note`, `rel-redact`), the typewriter's `/Copies` key and `<r>`
   redactions. No alpha anywhere: the paper is a field of opaque cells (fine at
   the edges), copies are a chain of transforms with clips, stamps are
   clip-with-holes. The library has its own generator (`rlU`), so it is
   identical in pscat and gs and never touches `rand`. Compare against
   `docs/dev/design/typewriter/release_vs_real.png`
   (`docs/dev/design/typewriter/tools/release_compare.py`); `notes/214-released-look.md`.
   #215 shipped the gallery piece `gallery/released_memo.ps` (*Released in
   Part*) and the blind boards `docs/dev/design/typewriter/blind_grid.png` and
   `memo_vs_real.png` (`docs/dev/design/typewriter/tools/blind_compare.py`; the
   key is in the PR, not the repo); `notes/215-released-memo.md`. #531 gave it its
   final pass: #590's even hand, two copies, an office-use-only marking ruled
   through, exemption codes, a review stamp, a document number and a gem clip, and
   redrew the blind board with a new shuffle seed (`notes/531-released-in-part-final-pass.md`;
   the new key is in a PR comment, not the repo). #590 added a second
   piece, `gallery/time_traveler_memo.ps` (*The Man from 2026*): a never-released
   TOP SECRET original (`/Generation 0`, legal paper), with the red banners and
   control lines typed by `tw-set` after `tw-type` on the same frame
   (the only real marking is TOP SECRET; the regrading box is invented);
   `notes/590-time-traveler-memo.md`. #466 gave a
   copy's paper a grey haze, mottle and grain (`rel-page /Haze`, default 0.6,
   applied after the copier curve in `rlSet`/`rlCol`; `/Haze 0` is the old white
   copy; `notes/466-copy-paper-haze.md`). #467 shipped (`notes/467-misstrike-y-tail.md`).
   #451 redrew `/Clip` as a gem clip of one wire (outer loop on the sheet, inner loop a dent, a rust ghost, `notes/451-paper-clip.md`). #554 made the mis-strike configurable: `/MisStrike` (rate, default 0.01) and
   `/MisStrikeScale` (size multiple of the 4x/2x, default 1), byte-identical at the
   defaults and not set by the presets (`notes/554-misstrike-keys.md`).
   #468 searched for
   two-colour, embossing, worn-lane and confirmed-electric scans and found none:
   `notes/468-typewriter-trait-references.md`. #474 brought the
   haze's fine grain to the scans' strength with two `imagemask` stencils per
   16pt square (`rlHazeGrain`, on `rlFieldSet`'s field colour;
   `notes/474-copy-paper-haze-grain.md`). #528 gave the mottle a crumpled-paper
   wave: four tilted, stretched noise octaves (no axis-aligned lattice), a few
   long creases (`rlHzCrease`) laid before the grain, and the grain's squares
   cut to 8pt (`notes/528-copy-paper-wave.md`). #449 added `rel-page /Dropout`
   (a field of holes in a clip, `notes/449-thin-stroke-dropout.md`); #547 retired
   it for per-glyph erosion in `tInkStrike`: on a copy the feather and body are
   drawn inside the glyph grown by the spread (`tEr`) and shrunk by `tEy` in y
   only (`tGrowClip`, `tECopy`), so thin horizontals go and stems stay; the
   hole clip and the bar replay (`rlBarsAgain`) went with it
   (`notes/547-copy-outline-erosion.md`). #450 added `rel-page /Soften` and a
   per-stamp / per-note `/Soften`: inside a copy the marks get a fringe of five
   stepped rings (the outline stroked wider, in the ink mixed toward the paper),
   scaled by the copy loss; no randomness, an original is unchanged
   (`rlStampRings`, `rlBarRings`, `notes/450-copier-softening-of-marks.md`).
   #595 added the typewriter's `/InkMap` (a proc `r g b -> r g b` run on `/Ink` and
   `/BleedInk` just before `setrgbcolor`; default `{ }`), which `rel-page` sets, with
   `/Copies`, to `{ RelState begin rlCol end }` for the content, so typed ribbon colour
   goes through the copier's curve (`notes/595-copy-ribbon-colour.md`). It changed the
   bytes of every committed render of a copied `rel-page` (near-black ink becomes pure
   black under the curve).
   #448 made `rel-page` multi-page: `tw-type` has a `/PageEnd` hook (as `/PageStart`, after every page), and `rel-page /Paged true` with `/PageStart { rel-page-start } /PageEnd { rel-page-end }` draws each sheet whole (paper, copy layers, marks; the hooks leave the page's frame set by popping and re-pushing tw-type's own gsave); `notes/448-released-multipage.md`.
   #423 tuned the ribbon ink toward T2, T4 and T7: a thinner body and feather
   (`wBodyW`, `fW` in `tInkStrike`), a higher base hole level, a steeper
   pressure-to-density slope, pressure sd 0.12 on `/manual` and `/worn`, and a
   very-light strike drawn from 0.3 to 0.7 deep. The defaults changed, so the
   typewriter and released renders were redone; `notes/423-ink-lighter-uneven.md`.
   #477 made a light strike thin rather than only dot out: the body is clipped to
   the glyph shifted four ways (an erosion, `tErode`), by an amount that grows with
   the pressure shortfall, and the hole slope below pressure 1 fell from 3 to 0.6.
   A copy (`/Copies > 0`) is unchanged. The typewriter and released specimens were
   redone; `notes/477-light-strike-thins.md`.
   #511 gave elite its own ink weight: at `/Pitch` 12 the hole level is higher by
   `tEliteHole` x `/Ribbon` (`tFaceK` is 0 at pitch 10, 1 at 12), against T4's elite
   being about 6% heavy. Pica and a photocopy (`/Copies > 0`) are byte-identical.
   Only the typewriter specimen (pscat and gs) and `ink_vs_real.png` were redone;
   `notes/511-elite-ink-weight.md`.
   #520 raised `/worn`'s `/Ribbon` from 0.35 to 0.55 (T10's worn elite was x0.66 of
   its scan by dark pixels per character, now x0.76); `/manual` and `/electric` are
   byte-identical; `notes/520-worn-elite-weight.md`.
   #422 added two opt-in ribbon extras, both off by default so a page without them is
   byte-identical: `/WornBand` (the lane every x-height strike hits prints lighter) and
   `/Bleed` with `/BleedInk` and `/BleedSide` (a black/red ribbon's other colour past a
   wandering boundary, ragged at the weave). Both are bands at a fixed height in the
   cell (`tYoff` taken out), drawn as overlapping passes by `tRun` / `tZoneDo`. The
   specimen has a new "ribbon extras" block; the bleed is unsourced (no two-colour
   reference on the board, and #468's wider search found none; NBS C431's text is the
   only source); `notes/422-ribbon-ink-extras.md`. #538 added the opt-in `/BleedGap`
   (a white pin stripe at the boundary, in yarns, default 0, byte-identical) and
   documented `/WornBand`'s dark-tops sign as a look, not a measurement; no scan
   was found, so nothing was tuned against one (`notes/538-bleed-gap-and-worn-band.md`).
   #539 routed a clogged counter's fill (`tClogBand`) through `tRun` / `tZoneDo`
   like the feather and body, so it takes the bleed colour, the worn lane's lighter
   weave and the pin stripe where the zones put it; with both extras off the code is
   the same operators (`notes/539-clog-bleed-zone.md`).
   #242 shipped the charcoal gallery piece, `gallery/lincoln_charcoal.ps`
   (*Study after Berger, 1864*), and `lib/etching.ps`'s `et-tonemap`, which
   returns a photo as a grid of darkness for any piece that wants a
   photo's tones as data. The piece passes its tone map to `dhhatch` as a
   `/Value` procedure. It renders in about 23 s in pscat and 22 s in gs, against
   a 60 s budget. Ghostscript needs `--permit-file-read` for the libraries and
   the JPEG. `notes/242-lincoln-charcoal.md` lists what the drycharcoal tools
   could not do: a smudge or lift shows its own outline wherever the
   hatching over it is light, and the hatching is crisper than real
   stumped charcoal.
5. Watercolor (issue #47, gated on the architecture spike in issue
   #46): read `docs/dev/design/WATERCOLOR.md` before starting — it recommends a
   small `alpha: f32` field on `GraphicsState` (already prototyped as
   `pub(crate)`-only in `src/gfx.rs`'s `tests::
   watercolor_prototype_b_alpha_sample`, not yet a public operator) as
   the primary mechanism, plus the SVG/PDF export work that prototype
   deliberately left undone, and names a real gs-portability gap (no
   PostScript-callable alpha operator in gs 10.07.1) to document rather
   than paper over.
6. Shrinking PS-library-only coupling to Rust/CI/gs (architecture
   spike in issue #92): **read `docs/dev/PS_LIBRARY_COUPLING.md` in full
   before starting either of its two remaining follow-ups (a phased
   PS-native verification path, CI diff-shape detection) — do not work
   from a summary of it, including this one.** That document itself
   needed eight rounds of cross-model review to converge (mostly the
   same failure mode each time: a correction made in one section not
   propagating to a shorter restatement elsewhere, including in an
   earlier version of this very docs/dev/HANDOFF.md entry) — a short paraphrase
   here is exactly the kind of restatement that risks going stale the
   next time the source document is revised. Read the whole thing.
   The first follow-up, a doc-comment-driven capabilities catalog, is
   done (issue #94): new `% @kind:`/`% @summary:`/`% @example:`/
   `% @param:`/`% @internal`/`% @requires:` doc-comment tags in
   `lib/*.ps`, parsed at build time by the new `build.rs` into
   `src/capabilities.rs`'s catalog — see `build.rs`'s own module docs
   for the tag grammar and docs/archive/NOTES.md's issue #94 entry for the full
   story. `lib/paintkit.ps` and `lib/hatchkit.ps` (issue #49 — a
   brand-new file, so it started tagged rather than being added to
   `LEGACY_FILES`) are migrated so far; migrating the rest of
   `lib/*.ps` — `artkit.ps`/`pagekit.ps`/the four style packs/
   `handscript.ps`/`hangul.ps` — is itchy-when-you-get-to-it follow-up
   work, not a filed issue. The mechanism already
   handles `Template`/`Dial` generically (same `/name ... def`
   discovery as `Procedure`) — including `lib/styles/*.ps`'s
   `/name /othername def` shape (a Dial bound to another name literal,
   e.g. `/spmetal /brass def`) and `bind def`, both needing a real
   redesign of the definition-name tokenizer to get right, not a
   one-line fix — two rounds of Codex review on PR #97 caught two
   different ways a "which literal is the name" heuristic broke on
   real `lib/styles/steampunk.ps` code; see `build.rs`'s
   `find_top_level_defs` docs and docs/archive/NOTES.md's issue #94 entry for the
   full story before touching that function again.
   Two kinds still need new discovery logic before their file can
   migrate: `Palette` (`Palettes /name [...] put` dict-literal
   mutations, not `def` bindings) and `Type3Face` (`/Name Dict
   definefont pop`, not `/name ... def` either — `@kind: Type3Face` is
   explicitly rejected by `build.rs` until this exists, so
   `handscript.ps`/`hangul.ps` can't migrate yet regardless of
   `Palette`). Migrating a file needs no new test either —
   `tests/capabilities.rs`'s `every_migrated_file_names_
   match_the_catalog_exactly` cross-checks every file
   `capabilities::migrated_files()` reports, generically.
   The second follow-up, a PS-native verification path, has its
   **Phase A** done (issue #95): `%%SelfTest` doc-comment blocks run by
   `pscat --selftest`, plus a strict `--lint` mode that actually fails
   the process, run against rendering drivers under `selftest/drivers/`.
   `./scripts/selftest.sh` runs both, and CI runs it as its own step.
   **Read `docs/dev/SELFTEST.md` before adding self-tests or a driver** —
   same reasoning as above: it records the assertion vocabulary, why
   there is deliberately no "assert something raised" form, and (with
   `docs/dev/GS_CHECK_INVENTORY.md`) exactly which defect classes stay
   uncovered and why. Two pieces remain open, both filed:
   **Phase B** (#134), a pixel-sample operator for the
   geometry/measurement class Phase A structurally can't reach, and the
   `ghostscript_accepts_*` extraction (#135), which #95 inventoried (25
   drivers today, not the decision record's 16) rather than performed.

## Gotchas for the next implementer

- **Fills don't use tiny-skia's anti-aliasing** (#692). `fill`, `eofill`,
  `rectfill` and glyph fills rasterise through `src/coverage.rs`, an
  exact-area accumulation rasteriser. Its coverage goes into a page-sized
  mask that `Gfx::fill_coverage` composites with `fill_rect`, AA off, one
  call per run. Fully covered runs go without the mask. tiny-skia's own path
  AA supersamples 4x4, so a shape thinner than a quarter pixel vanished.
  Strokes, clip masks and `shfill` still use tiny-skia's rasteriser. A
  consequence for libraries: a 0.01 pt overlap meant to hide a seam is now
  visible as a 1-2% tint (`lib/drychalk.ps`'s tray had one). Overlap away
  from what must stay clean. See `notes/692-subpixel-fill-coverage.md`.

- **Every paint runs tiny-skia's f32 pipeline** (`force_hq_pipeline`, #731).
  The default lowp pipeline rounds each blend with a bias, so a stack of
  faint layers (low `setalpha`, or sub-pixel coverage) stalled early. A new
  `Paint` built anywhere in pscat must set the flag too, or its pixels drift
  by a level from the rest. Storage is still 8-bit, rounded per blend (the
  #731 part 2 float buffer is open). See `notes/731-hq-compositing-pipeline.md`.

- **Transcendental math goes through `src/fmath/`, never `f64::sin` and
  friends** (#738). The platform libm rounds differently on macOS, Linux and
  wasm, and seeded noise/erosion grows that last bit into visible pixels
  (`gallery/mountains.ps` in fractal_clouds: 25,000 of 64,000 pixels differed
  between a Mac and Linux). `fmath` is musl 1.2.5's math on every target: the
  pure-Rust `libm` crate (pinned exactly in `Cargo.toml`) for trig, `log10`,
  `hypot` and the `f32` ones, and a port in `src/fmath/musl.rs` for `exp`, `ln`
  and `powf`, because the crate has the older FreeBSD versions of those three.
  `clippy.toml` bans the direct methods and `Transform::from_rotate`, so clippy
  fails a new one (a test that only needs a reference value may
  `#![allow(clippy::disallowed_methods)]`). Need another function? Check it bit
  for bit against musl first (`scripts/fmath_check.sh`): `libm`'s `cosh`,
  `cbrt`, `log2` and `exp2` do not match, so they are not offered. Not covered:
  tiny-skia's own f32 `acos`/`cos`/`powf` (the cubic-root finder in stroking of
  nearly straight cubics, gamma stages), which use the platform libm; see
  `notes/738-fmath-one-libm.md`. `src/fmath/tables.rs` is generated
  (`scripts/gen_fmath_tables.py`), do not edit it.

- **Handwriting is worded as a style, not as passing for real** (#686).
  Say "handwriting-style notes" or "a natural, hand-drawn script" in the
  skills, the plugin README and the MCP tool description, never
  "human-looking" or anything about passing as real handwriting, and keep the
  one purpose statement (hand-drawn styles for notes, cards and art; not for
  imitating a specific person's handwriting or presenting writing as someone
  else's). The `pscat` skill tells the agent to decline a named person's hand
  and offer a generic one. The sloppy-but-legible glyph guidance in
  `docs/guides/handwriting.md` and `lib/handscript.ps` is unchanged on purpose.
  The `plugin.json` manifests carry no such claim, so they were not edited.

- **Workflows: every action is pinned to a commit SHA, and only the
  `release` environment may hold `id-token: write`** (#602).
  `python3 scripts/check_workflows.py` (CI's `release-policy` job) fails a
  `uses: x@v4` or an OIDC grant anywhere else (the Pages deploy is the one
  allowed exception). Resolve a new pin with
  `gh api repos/OWNER/REPO/commits/TAG --jq .sha`, and write the tag in a
  comment. The release archives are named by target triple
  (`pscat-X.Y.Z-aarch64-apple-darwin.tar.gz`), and the Linux musl ones are
  headless (`docs/dev/RELEASING.md`, "Platforms").
- **A release `pscat` reads its compiled-in `lib/`, not the checkout's,
  unless it runs from the checkout root** (#598). `(lib/x.ps) run`
  tries `$PSCAT_LIB`, `$PSCAT_HOME`, the cwd, the user data dir and the
  exe's dir before the embedded copy, and only debug-assertion builds
  (debug, `--profile ci`) fall back to the checkout. So a lib edit is
  live for anything run from the repo root (tests, `identity_check.sh`,
  `gallery/show.sh`) but not for `target/release/pscat` run elsewhere
  until you rebuild or set `PSCAT_HOME`/`PSCAT_LIB`; the scripts in
  `scripts/` that read the font catalog (`build_site.sh`,
  `build_font_gallery.sh`) export `PSCAT_HOME` to their own tree for that reason.
  `PSCAT_TRACE_RESOURCES=1` shows which copy served each file. The
  order is documented once, in `docs/guides/getting-started.md`.
- **handwrite, typewrite and photo-etch are `pscat` subcommands (#600).**
  `src/subcmd/` (one module each, dispatched from the top of `main`)
  builds the PostScript driver (the library's bytes from `paths::open`,
  then the options dict) and renders it through `run_headless` /
  `finish_headless`, so the output is what `pscat --png driver.ps` makes.
  Keep it that way: don't reimplement rendering there. A numeric option
  must pass `subcmd::number` and its original text is what lands in the
  driver (`36` and `36.0` differ in PostScript); never splice a raw
  option. The `scripts/*.sh` of the same names are thin wrappers, and
  `pscat-mcp`'s `handwrite` runs `pscat handwrite`. See
  `notes/600-tool-subcommands.md`.
- **Interpreter execution limits (#604).** `Limits` (`src/limits.rs`) via
  `Interp::set_limits`; CLI `--max-steps/--max-time/--max-memory`; on by default
  in `pscat-mcp`. The design points that are easy to break:
  - The step cap costs nothing per step: `step_n` (via `limit_budget`) shrinks
    the budget it hands `step_n_inner` and `after_slice` adds it to the count
    afterwards. Keep `step_n_inner` with ONE call site and the new `PsError`
    variants payload-free: either change cost 2 to 12% in instructions retired
    (bisected on `fillkit.ps`, `/usr/bin/time -l`); compare instruction counts,
    not wall time, when you touch that function. A loop with an
    empty body never yields an object, so `next_item` calls `tick()` on every
    `Iterate*` action to poll the limits itself; a new loop-like frame must tick
    too, or `{ } loop` hangs again.
  - A whole-file read (`file`, `run`) is inside one operator, so it polls the
    limits itself (#702): `ops/file.rs` `read_resource` takes
    `file::read_path_limited` (64 KiB chunks, a helper thread for anything that
    is not a regular file) through `paths::open_with_reader` whenever limits
    are set, and `Interp::poll_read` charges a step per KiB and checks time and
    memory. A new operator that reads an unbounded source must do the same;
    with no limits the plain `std::fs::read` path is untouched. See
    `notes/702-whole-file-reads-and-limits.md`.
  - `PsError::StepsExceeded`/`TimeExceeded`/`MemoryExceeded` must stay uncatchable: `recover` returns it before
    looking for a `stopped` frame, and it is never written to `$error`. A new
    place that swallows operator errors must let it through.
  - The memory cap counts process heap growth from `pscat::TrackingAllocator`
    (`src/memtrack.rs`, `#[global_allocator]` in `main.rs`). A new binary or test
    that wants a memory limit installs it; `set_limits` refuses otherwise.
    Memory tests live alone in `tests/limits_memory.rs` because the count is
    process-wide. An operator that can allocate a lot in one step should call
    `Interp::check_alloc(bytes)` first, as `array`, `string` and `aload` do.
  - Time reads `crate::clock`, never `Instant` (wasm32), so it is a no-op there.
  - Fuzzing: `fuzz/` (own workspace; `cargo +nightly fuzz run interp fuzz/corpus/interp fuzz/seeds/interp`),
    harness bodies in `src/fuzzing.rs`, seeds and crash regressions replayed by
    `tests/fuzz_corpus.rs`, weekly in `.github/workflows/fuzz.yml`. A fixed crash's
    input goes in `fuzz/regressions/<target>/`. Seven targets (#604, #704): `lexer`,
    `interp`, `truetype` (the font-file loader and glyph outlines; `font::prepare_face`),
    `type1_charstring` (decrypted charstring bytes into `type1::run`), `type1_font` (a
    whole PFA through `eexec`/`definefont`, then text in every defined font), `filters`
    (ASCIIHex/85, RunLength, eexec, Flate, LZW, DCT, chained) and `image` (a `%!`
    program runs as PostScript; anything else is a recipe that builds an
    `image`/`colorimage`/dict/multi-source/ImageType 3 call). A new loader gets a
    function in `src/fuzzing.rs`, a `fuzz/fuzz_targets/` file, a `[[bin]]`, seeds,
    a replay test, and a line in the workflow's matrix (an owner path). The replay
    runs each input on a thread under a 30 s deadline, so a hang fails the test.
    A font loader must use `font::parse_face`, not `Face::parse`. Not fuzzed:
    `readreals` (a path API; its parser is `from_le_bytes` on exact chunks).
  - `--spool` and the subcommands take the limits (#701): one budget per
    program run (`subcmd::Confine`, `take_limit_flag`, `limit_flag` in
    `main.rs`, `window::run_spool_with_limits`, `chalk::run_with_limits`).
- **File writing (#740).** Off by default: `Interp::set_write_roots` (CLI
  `--allow-write DIR`) names the directories, and `src/write_roots.rs` is the
  one place that decides whether a path is writable (`WriteRoots::open`,
  `resolve_entry`; `Interp::writable_path` for new writers such as #733/#735).
  Never open a file for writing anywhere else, and never add an env-var form of
  the flag: `pscat-mcp`'s children inherit the environment, and the MCP layer
  must never write from PostScript (`tests/file_write.rs` asserts it). Output
  files are `file::Sink`s; `%stdout` goes through `Interp::write_stdout` so a
  captured stdout keeps order with `print`. Disk writes are buffered (#762,
  `file::DiskSink`, 64 KiB): the interpreter keeps weak handles to the open
  files (`Interp::flush_outputs`, cap `MAX_OPEN_OUTPUT_FILES` = 64) and the CLI
  flushes them on every exit path (`flush_outputs_or_report` in `main.rs`,
  `window.rs`), because it never tears down the dictionary graph that may hold
  an open file. A new CLI exit path must flush too; `Drop for Interp` is only the
  backstop. `--max-write-bytes` is `Interp::set_max_write_bytes` (1 GiB with
  `--allow-write`). See `notes/740-file-writing.md` and
  `notes/762-file-hardening.md`.
- **`pscat-mcp` limits from the environment (#756).** Every numeric flag has a
  `PSCAT_MCP_<FLAG>` variable, from one table (`NUMERIC_OPTIONS` in
  `src/mcp_server.rs`) that also holds the ranges and defaults, so the two
  cannot drift. Env is read after the flags (so `--help` ignores it), a flag
  wins, a bad value is a startup error. Never add a variable for
  `--allow-read` or `--out-root` (same rule as #740); a hosted server (#689)
  must not read these. Tests build a `Config` with an injected lookup, never
  `std::env::set_var`. See `notes/756-mcp-env-limits.md`.
- **`--bands N` (#736).** One PNG rendered as N horizontal bands on N threads
  (`src/bands.rs`, a binary module), each a full run with `/BandIndex`,
  `/BandCount`, `/BandBBox` in userdict, stitched by whole device rows. Byte
  identity with a plain render rests on one rule: the only per-band shortcut
  allowed is one that leaves the band's own rows exactly as a full render paints
  them. Today that is just image rasterization (`Gfx::set_band_rows`, read in
  `image.rs`). Never add a PostScript-visible clip or a tiny-skia mask for it. A
  new shortcut (e.g. culling fills whose bbox misses the band) needs a provably
  conservative bound and a run of `tests/bands.rs`. See
  `notes/736-bands-parallel-render.md`. Since #844 bands are queued work units,
  not threads: `bands::plan` cuts K bands (default 2 per thread, min 8 rows;
  `--band-rows`) and the N threads pull them from an atomic counter, so the
  band names describe the queued band. `--max-memory` uses one shared base
  (`MemoryBase`), which replaced #736's two start gates. The tracking
  allocator batches its count per thread (`memtrack.rs`, 64 KiB): a shared
  atomic per allocation cost band threads ~2.8x CPU. Don't put a per-allocation
  shared atomic back. See `notes/844-bands-work-queue.md`. The band names are
  plain `userdict` entries, so a program that hashes, serialises or `forall`s
  userdict sees per-band values: fractal_clouds' bake cache keyed on userdict
  and missed in every band ([#844 re-test](https://github.com/PSCatLabs/pscat/issues/844#issuecomment-6429744992)).
  The docs tell programs to leave them out of such keys. The preamble
  runs inside `name::detached` (#868): its new names take ids from a separate
  range and no sequential slot until the program first interns them, so the
  program's own names keep a plain run's ids and `forall` order. A new
  per-run preamble that defines names should do the same
  (`notes/868-band-preamble-name-ids.md`). `--sweep NAME=` and `pscat sheet
  --sweep` now do, and every sweep frame and sheet cell runs on a fresh thread
  (`run_on_fresh_thread` in `main.rs`), so names an earlier frame made with
  `cvn` don't shift the next frame's ids (#891,
  `notes/891-sweep-frame-name-isolation.md`). Each `--spool` job runs on a
  thread of its own too (`window.rs` `JobThread`, #895): the event loop
  can't hand the job to `run_on_fresh_thread`, since it steps it a frame at
  a time, so it sends the job's thread one call per step or draw and waits
  (`notes/895-spool-fresh-threads.md`). A new loop that runs several
  programs in one process and promises standalone output needs the same. The
  backup-task / `--band-cuts` work for the tail (#869) was measured, then
  shelved once the re-test showed the queue already beats a cost-balanced
  split there (data on #869). `pscat-mcp` exposes it as a `bands` argument on
  `render_postscript` and `start_render` (#843), capped by `--max-bands`;
  a banded run drops the server's `--progress`, which `--bands` refuses, but
  passes `--lint` again since #847 (see below). `--band-rows` is deliberately
  not exposed. See
  `notes/843-mcp-bands.md`. Bands above 0 also drop stderr
  (`Interp::quiet_stderr`; every program stderr write goes through
  `Interp::write_stderr`, so a new one must too), and their font notes are
  printed once at the end (#877, `notes/877-bands-stderr-once.md`).
  `--lint`/`--lint-strict` work with `--bands` (#847, `notes/847-lint-with-bands.md`):
  band 0 runs `lint::check_band` in its own thread (the `Interp` can't leave
  it) and `bands.rs` runs the blank-page check on the *stitched* pages with the
  ink flag OR'ed across bands, because a band-aware program leaves its own
  canvas empty outside the band. The output is one report after the run
  headed by a `pscat: lint: note:` line (not a finding, so not counted by
  `--lint-strict`); `pscat-mcp`'s `split_lint_findings` keeps the note only
  beside findings. A new lint check that reads the canvas must go through
  the stitched pages, not `check_band`.
- **`--profile` (#737).** Per-procedure call counts and inclusive/exclusive time
  on stderr (`src/profile.rs`, a `Profiler` on `Interp`). `execute_resolved` takes
  the name a body was reached through (`via`, `NO_NAME` for `exec`/`if`) and, with
  profiling on, `prof_call` plants a `Frame::ProfReturn` marker under the body's
  `Proc` frame. The marker is reached after the body and its tail call (a `Proc`
  frame is popped *before* its last element runs, so the frame itself cannot mark
  the return), and `cleanup_unwound_frame` / `exit_loop` close it on `stop`,
  errors, `exit` and `quit`. A call to a name already among the markers on top of
  the exec stack plants none (tail recursion stays at constant depth; non-tail recursion overflows the exec
  stack at about half the plain depth, since a marker takes a slot). Keep the
  hot path as it is: the only cost off is one `profile.is_some()` in
  `execute_resolved`'s `Array` arm (plus, from #799, one operators-on check per
  `step_n` slice), and everything else is `#[cold]`. A first
  version that made `step_n_inner` generic, or added a second caller of
  `execute_resolved`/`push_proc_frame`, slowed a plain `fib` run by 19%; measure
  with `scripts/bench_compare.sh` after touching `interp.rs`. See
  `notes/737-pscat-profile.md`. #799 added `--profile-graph` (caller -> callee
  edges, gated by `Profiler::graph`, recorded in `enter`/`exit`/`count_only`),
  `--profile-json PATH` (main.rs: pre-checked and written through
  `Interp::writable_path`; the path is the next argument only if it ends in
  `.json`), and `--profile-ops` (per-operator time: `step_n` routes to
  `step_n_timed`, a copy of the loop, once per slice when it is on; operators
  are bare `fn` pointers, so `proc_step::<true>` leaves the operator's name in
  `Interp::op_name` and rows are keyed by `Operator::name`, #885). The
  copy is a second caller of `next_item`/`execute_resolved`/`push_proc_frame`/
  `recover`/`execute_element`, which is what cost 19% in #737 until those were
  made `#[inline(always)]`; re-measure `fib` with `bench_compare.sh` if you
  touch them. See `notes/799-profile-followups.md`.
- **Read roots (#763).** Reads are open by default (the CLI). `Interp::set_read_roots`
  (CLI `--read-root DIR`, `--confine-reads` for none) confines them, and
  `src/read_roots.rs` decides: `open_confined` wraps the search order's reader so a
  disk candidate outside the roots is skipped (the embedded `lib/` still answers) and
  a name that only resolved outside is `invalidfileaccess`; `filenameforall` filters
  through `ReadRoots::admit`. Every operator that reads a program-named path must go
  through `ops::file::read_resource` (or `open_confined`), never `std::fs` directly.
  `pscat-mcp` always passes `--confine-reads` plus its roots (cwd unless `/`,
  `$PSCAT_HOME`, the per-user data dir, `--allow-read`) to `pscat` and `pscat
  handwrite` children. Font lookup matches names against file stems and isn't gated.
  See `notes/763-read-roots.md`.
- **`writereals` / `readreals` (#735).** Packed little-endian float64 (or `/f32`) save and
  load of a numeric array, in `src/ops/realsio.rs` (module doc has the error table and the
  `/f32` rules). The writer opens through `WriteRoots::open` like `file` does, and checks every
  element before opening, so a typecheck never truncates. It reads elements with
  `PsArray::peek` (never `snapshot`, which would be an unaccounted copy) and encodes through
  one fixed 64 KiB buffer; the reader `check_alloc`s file plus array before reading. A short file
  is `rangecheck`, not `ioerror` (the PLRM keeps `ioerror` for non-EOF failures). Tests:
  `tests/realsio.rs`, and `tests/realsio_memory.rs` (own binary, tracking allocator). See
  `notes/735-readreals-writereals.md`. #773 added `(path) /all readreals`, `(path) offset n
  readreals` (offset in values) and the file-object forms `array file writereals` / `file
  n|/all readreals`, in the same module (`tests/realsio_files.rs`;
  `notes/773-readreals-all-offset-file.md`). The bulk read is `PsFile::read_into`, which keeps
  `consumed`/`line` exactly as `read_byte` does (for a filter too since #896; the `filters`
  fuzz target reads every chain both ways and asserts they agree).
- **`pscat sheet` composes finished renders (#693).** Unlike the subcommands above it
  builds no driver: it renders each file itself (a probe `Interp` gives every cell's
  pixel size first), lays the cells out with `contact_sheet::Layout` (pure geometry, so
  an oversized sheet fails before any cell renders), and draws labels with
  `contact_sheet::render_text`, which is PostScript run through `Interp`. The old
  `--contact-sheet` path (`run_sweep` in `main.rs`, `new_sheet`/`blit_cell`) is untouched
  and must stay byte-identical. See `notes/693-contact-sheet-labels.md`.
  `render_text` picks the face per character by script (#707): Latin in Helvetica
  (accents through `ISOLatin1Encoding`), Hangul/kana-kanji/Thai in the Unicode-mode Noto
  faces of the `cjk-thai` pack (missing pack: `?` and one stderr warning, not a failed
  sheet). A Latin-only label keeps the pre-#707 pipeline to the byte. See
  `notes/707-sheet-non-latin-labels.md`.
  #706 added sweeps (`--sweep-seed`/`--sweep`, which call the flag CLI's private
  `parse_seed_spec`/`parse_sweep_spec`/`format_sweep_value` in `main.rs` from the child
  module), a per-cell mat (`DPI@COLOR`, `composite_on_mat`), stacked columns (`+`) and
  `--cols` wrapping, through `Layout::packed` and `PackedRow` (`Layout::new` delegates to
  it and is unchanged for single-cell columns). With none of the new options the sheet is
  byte-identical to before. See `notes/706-sheet-sweeps-mat-cols.md`.
- **`pscat-mcp`'s contract (#568).** The server lives in
  `src/mcp_server.rs`, pulled into both binaries with `#[path]` (not the
  library): `pscat-mcp` and `pscat mcp` are the same server (#569).
  `SUPPORTED_VERSIONS` there lists only revisions tested against a real client
  (2025-11-25: Claude Code, Inspector; 2025-06-18: Codex); never echo a
  client's version. `server/discover` must stay -32601 until 2026-07-28 is
  really implemented (#621): -32022 there tells a dual-era client the server
  is modern. Every child goes through `run_child` (timeout, 4 MiB per stream)
  and every `out_path` through `confine` + `Dest::write_pages` (temp file +
  rename; `confine` also requires the render's extension and refuses
  dot-directories, #627); a new tool that writes files or runs a child must too, and gets
  cases in `tests/mcp.rs`. Only JSON-RPC on stdout. Error results carry
  `structuredContent.error` beside the text; success results never do (some
  clients show `structuredContent` instead of `content`, and that would hide
  the image), except the job tools' own facts (`start_render`,
  `render_status`, `cancel_render`: JSON, no image; `render_result` returns
  the render's result as stored). Every tool carries all five annotation hints (#687) through the
  `annotations()` helper; `tests/mcp.rs` `every_tool_has_annotations` has a
  per-tool table and fails for a tool missing from it, so a new tool states
  its `readOnlyHint` (conservative `false` when an argument such as `out_path`
  decides). `scripts/mcp_inspector_check.sh` (workflow `mcp`) is the
  third-party client check. See `notes/568-mcp-skills-cleanup.md`.
  Input schemas keep unions as `anyOf` branches with one `type` each, never a
  `type` array; Codex and Claude Code were checked for real and
  `tool_schemas_use_only_the_portable_union_shape` pins it (#712,
  `notes/712-mcp-schema-codex.md`).
  **Concurrency and jobs (#622, #741).** The stdin thread answers
  `initialize`, `ping`, `tools/list`, `notifications/cancelled` and the job
  tools inline; every other `tools/call` runs on its own thread
  (`Server::spawn_call`), registered in `inflight` *before* it starts, and
  removed (with its cancel flag read, under the same lock as
  `cancel_request`) *before* its reply is written: tests reuse id 100 back to
  back. Replies go through `Server::send` only (one line under the stdout
  lock). Each run's `Config` copy carries its cancel flag, which `run_child`
  polls with the timeout; spawns are serialised by `SPAWN_LOCK` (macOS pipes
  are not created close-on-exec atomically). Jobs live in `JobTable` behind
  one mutex and the `job_wake` condvar (notify under the lock); `JobSlot`'s
  drop releases a slot even on a panic. `shut_down` cancels every job (calls
  in flight finish and answer) and joins every thread when stdin closes, and children get
  `--exit-with-parent <server pid>` for a server killed outright. Locks are
  never nested. Tests avoid elapsed-time assertions: they gate a render on a
  FIFO and count children with `ps` (macOS `pgrep -P` hides zombies). See
  `notes/741-mcp-async-render-jobs.md`.
  `chalkboard_math` (#688) is the pattern for a data-in tool: the program is
  generated in-process (`pscat::chalk::driver`, pure), then run in a `pscat`
  child through `run_child` with `interpreter_args`, never in the server
  (a program's stdout would corrupt JSON-RPC, and the child keeps the limits
  and crash isolation). Its caps (`--max-latex-bytes`, default 4096, and
  `--max-pixels`, default 40 M, shared with `render_postscript`, `handwrite`
  and `review_sheet`, #711; `Config::max_latex`/`max_pixels`) are checked
  before the child starts (`handwrite`'s height is auto-sized in the child, so
  the server passes `pscat handwrite --max-pixels` and maps its refusal,
  `pscat::limits::parse_pixel_cap_message`, to `output_too_large`). The canvas
  (4 bytes a pixel) is allocated before `--max-memory` counts, so the pixel
  cap is what bounds it below the interpreter's 8000-a-side ceiling. A TeX error is mapped back from the
  child's `mk-*` offending command with `chalk::explain`. The output is
  byte-identical to `pscat chalk` and `chalk::render` (`tests/mcp.rs` pins it).
  `review_sheet` (#708) is the pattern for a tool whose CLI twin cannot share
  one `--timeout` across cells (`pscat sheet` takes the limits since #701, but
  per cell, each with the whole budget), so the tool does not shell out to it. It renders each cell in its
  own child (`--page --dpi --png -` under `interpreter_args`, each child given
  what is left of the one `--timeout`) and composes the pictures in-process
  with `pscat::contact_sheet` (`Layout`, `render_text`, `blit_at`, `frame`,
  `zoom_nearest`). It repeats `src/subcmd/sheet.rs`'s composition loop and its
  two type sizes (`SHEET_LABEL_PX`, `SHEET_CAPTION_PX`), so a change to the CLI
  sheet's look must change both; `tests/mcp.rs`
  `review_sheet_is_byte_identical_to_pscat_sheet` fails when they drift. The
  fix that removes the copy is `--max-*` flags on `pscat sheet` and a shared
  composer in the library (see `notes/708-mcp-review-sheet.md`).
- **Published skills are `plugin/skills/<name>/SKILL.md`** (pscat, psart,
  psviz; #568; top-level `skills` is a symlink there since #569 part b), written for an installed pscat: no `cargo`, `target/`, `scripts/`
  or checkout-relative paths, repo files linked by GitHub URL. `.claude/skills/`
  holds the repo-development skills plus symlinks to those three, so a
  checkout's Claude Code sessions still load them. psart keeps a short
  `SKILL.md` and puts each medium in `references/`; add new media there.
- **The Claude Code plugin (#569).** The marketplace is
  `.claude-plugin/marketplace.json` at the repo root; the plugin is
  `plugin/` (`.claude-plugin/plugin.json`, a README, and `skills` as a
  symlink to `../skills`, so there is one copy of the skill text). Install
  copies only `plugin/` into the user's plugin cache, following the
  symlink; a repo-root plugin would copy all 136 MB. The MCP server is
  declared inline in plugin.json (`command: pscat-mcp`, found on PATH), not
  in a root `.mcp.json`, which would also register it for every checkout
  session. A SessionStart hook prints an install hint when `pscat` or
  `pscat-mcp` is missing. plugin.json's `version` must equal `Cargo.toml`'s
  (`scripts/plugin_check.py`, workflow `plugin`); the release script bumps
  it, and the marketplace entry carries no version. A new skill needs only
  a `plugin/skills/<name>/` directory. See `notes/569-claude-code-plugin.md`.
- **The Codex plugin (#569 part b) is the same `plugin/` directory.** The
  marketplace is `.agents/plugins/marketplace.json`; Codex reads
  `plugin/plugin.json` (portable manifest, version = Cargo's) and
  `plugin/mcp.json` (`type: "stdio"`, `command: pscat-mcp`); Claude Code
  ignores both. **Codex skips symlinks when it copies a plugin**, so the
  skills moved from `skills/` into `plugin/skills/` as real files (the
  old `plugin/skills -> ../skills` symlink installed zero skills in
  Codex); top-level `skills` and `.claude/skills/{pscat,psart,psviz}` are
  now symlinks into it, and `scripts/plugin_check.py` rejects any symlink
  inside a plugin directory. The missing-pscat SessionStart hook moved from
  Claude's plugin.json to `plugin/hooks/hooks.json`, the default path both
  agents read (never declare it in a manifest too: Claude loads both). Its
  logic lives in `plugin/hooks/check-pscat.sh`, run as `sh
  "${CLAUDE_PLUGIN_ROOT}/hooks/check-pscat.sh"` (the directory portal blocks
  an inline command in a subfolder plugin; Codex sets that variable too;
  #684, `notes/684-plugin-hook-script.md`).
  `plugin_check.py` also fails a plugin `.md` that writes a bundled or
  repo-relative image/font path in backticks or a code block (the
  directory checklist holds those; #848, `notes/848-plugin-no-image-paths-in-backticks.md`).
  `scripts/codex_plugin_install_check.sh` (job `codex-install`) installs it
  with the pinned codex CLI. See `notes/569-codex-plugin.md`.
- **The plugin icon (#683) is `plugin/.claude-plugin/icon.png`**, the
  1024 px render of `art/logo/pscat-logo-d.ps` (command in its header;
  512 and 48 px renders in `art/logo/renders/`). Claude's plugin.json sets
  `icon` to it (Anthropic's directory reads that field; Claude Code
  ignores it; it is not allowed in a marketplace entry) and Codex's
  `extensions.com.openai.interface` points `composerIcon` and `logo` at
  the same file. The directory takes the icon only once, at the first
  portal save or submit, so redrawing the logo later changes the file but
  not the listing. Never write the icon's path in backticks in the plugin
  README or a SKILL.md: the portal holds that for review. See
  `notes/683-plugin-icon.md`.
- **The MCP Registry entry (#569 part c) is root `server.json`**
  (`io.github.PSCatLabs/pscat`, case-sensitive: it must match the GitHub
  owner login, the README's visible `mcp-name:` line and the OIDC grant).
  One cargo package run as `pscat mcp`. The README line must stay visible
  text (crates.io strips HTML comments) and is frozen per crate version.
  The `registry` release stage publishes it last with the pinned
  `mcp-publisher`; `release.sh` bumps both version fields. See
  `notes/569-mcp-registry.md` and RELEASING.md.
- **Licensing (#563).** The code is MIT OR Apache-2.0. `Cargo.toml`'s
  `include` is an allowlist, so a new `include_bytes!`/`include_str!`
  target, or a file `build.rs` reads, must be added to it or the
  published crate won't build (`cargo package` verifies the build). A new
  tracked file under other terms (a font family, a reference photo, a
  crop) needs a row in `THIRD_PARTY_NOTICES.md` and in
  `licenses/assets.json`, where the first matching entry wins and anything
  unmatched falls to the code licence. Gallery and example art is CC0, but
  code copied in from `lib/` keeps the code licence.
- **CI checks beyond `test` (#601).** `cross-platform (ubuntu/windows)`,
  `msrv`, `packaging`, `licences` and `deny` are separate jobs in
  `ci.yml`; only `test` is required. `deny.toml` is the dependency licence
  allowlist plus advisory ignores (each names the issue that retires it).
  A new font directory under `fonts/catalog/` needs a licence file in it
  and an `assets.json` entry (`python3 scripts/check_licences.py all`).
  A file added to Cargo.toml's `include` must have an `assets.json` entry
  that ships in `crate` (`python3 scripts/check_licences.py package`).
  Windows CI traps: a temp path inside a PostScript string literal loses its
  backslashes (`C:\Users\x` reads as `C:Usersx`), so tests use
  `common::ps_path`; a spawned pscat binary needs `EXE_SUFFIX`; and a
  `/abs/x.ps` literal is not absolute there.
- **The crate's public API is three types (#565).** `Interp`, `Page` and
  `PsError` (modules `interp`, `page`, `error`) are the documented,
  semver-tracked surface, and `#![deny(missing_docs)]` holds them to it.
  Every other `pub mod` in `src/lib.rs` is `#[doc(hidden)]`: public only so
  the binaries, the wasm build and the tests reach it. A new module goes in
  hidden unless it is meant for library users. A new `pub fn` on `Interp`
  needs a doc comment or `#[doc(hidden)]`, or the build fails. `missing_docs`
  does not look inside hidden modules, and it does not check a re-export
  of a hidden item either, so supported items must live in a visible module.
  The `window` feature (default) gates `src/window.rs`; without it
  `src/window_stub.rs` stands in with the same signatures, returning an
  error. The crate is rlib-only: `scripts/build_wasm.sh` asks for the cdylib
  with `cargo rustc --crate-type cdylib`, because a cdylib in `crate-type`
  was also built by every downstream crate. `scripts/check_package.sh` runs
  the crates.io checks against the packaged crate: the size, a dry run from
  the unpacked tree, a `default-features = false` downstream build with no
  winit, and `cargo install`. CI runs it as `crate (os)`, alongside `docs`
  and the informational `semver` job.
- `PsArray` has a custom `Drop` (iterative teardown of last-handle
  storage; it moved off `Object` in Stage 11 for speed, so
  `obj.value` is movable these days). Don't reintroduce a
  `Drop for Object` — profiling showed it taxing every popped
  operand (~18% of fib).
- `Interp` split-borrows fields in `next_item` (`estack`, `dstack`,
  `gfx`, `ostack`); new frame types plug in there and must decide
  their action before any stack mutation (the `Action` enum).
- **Procedure elements are dispatched by `proc_step`, not `next_item`**
  (#323, #863). Both step loops (`step_n_inner`, `step_n_timed`) look at
  a `Frame::Proc` on top first: the element is inspected in place with
  `PsArray::peek` (one borrow, released before anything executes, so a
  `put` into the running procedure still works); a literal is pushed
  right there (numbers copied by `Object::clone_inline`), and an operator
  or executable name comes back as a fn pointer or interned id. The frame
  is popped as it yields its last element, before that element runs.
  `next_item` never runs a procedure element: when a loop iteration or a
  finished frame leaves a `Proc` on top, it returns `Item::Proc` and the
  loop calls `proc_step` in the same step, so `step_n(1)` granularity
  and the step limit are unchanged (`tests/dispatch_semantics.rs` pins
  step counts and error states). Don't turn the fetch back into
  `body.get(pc)`; that clone and its drop were the largest cost of a
  bound library run. In the same spirit, code holding a `PsName` must not
  go through `&str` (`define`, `load`, `Dict::put`): that allocates and
  re-interns. Use `define_name`, `load_id` and `put_name`.
- **Operator fast paths must succeed or defer, never fail** (#863). The
  arithmetic and relational operators (`binary`, `div`, `relational`,
  `eq`/`ne`) first look at the top two operands with `top2_nums` without
  popping; two numbers (a nonzero divisor, no NaN) are combined in place
  with `replace_top2`. Anything else runs the general path on an
  untouched stack, which pops and checks one operand at a time, so a
  failed `1 (a) add` still leaves `1`. A new fast path must keep that
  shape. `Object::drop_inline` skips the out-of-line `Value` drop glue
  for plain data; `PsArray`'s `Drop` checks the refcount inline and only
  the last handle's teardown is out of line. Measure such changes in
  instructions retired (`/usr/bin/time -l`), not time: two attempts that
  looked obvious (resolving names under the dict borrow, an inline `Array`
  arm in `clone_inline`) made the hot loop slower through inlining and
  register pressure (`notes/863-dispatch-cost.md`).
- **Fused conditionals read the live body; there is no compiled form**
  (#864). In a procedure, `{ } if` and `{ } { } ifelse` with a boolean
  under the first literal run as one step (`Interp::proc_literal`,
  `fused_conditional`). The elements are read from the array at that
  moment, so `put`, `putinterval`, an aliasing `getinterval` view and
  `restore` need no invalidation. Keep these rules if you add a fusion:
  - Fuse only where the run cannot fail (the operand types are checked
    first, and room on the execution stack), so every error comes from
    the unfused elements.
  - Charge each element to `slice_left`, and fuse only when the slice
    has that many steps left, so `step_n(1)`, slice ends and
    `--max-steps` count as before.
  - Never fuse in `step_n_timed` (`--profile-ops` counts operators).
  - Recognise an operator by its registered name (`runs_operator`),
    never by `fn_addr_eq`: function addresses are not unique, and a trial
    fusion matched in the test binary but never in the release build.
    The same rule holds everywhere: `eq`/`ne` on operators and an
    operator as a dictionary key go by `Operator::name` too (#881,
    `notes/881-operator-identity-by-name.md`). Several operators share one
    function body (`readonly`/`noaccess`), so address identity was wrong
    even in a build where addresses were stable.
  - The literal reaches the plain loop as `ProcStep::Op(proc_literal_op)`,
    with no arm of its own: a new arm in `step_n_inner` cost every element
    about 5 instructions.

  A fusion that pulls a common literal (a literal name) out of
  `proc_step`'s inline push costs more than it saves: `/name exch def`
  was measured and dropped (`notes/864-fused-conditionals.md`).
  `tests/dispatch_semantics.rs` pins every case with a positive control
  (`Interp::fused_steps`).
- **Loop bodies keep their per-iteration procedure frame** (#875,
  measured and dropped). Running a `repeat`/`for`/`loop`/`forall` body in
  place saves only 20–35 instructions an iteration: the `Rc` clone and the
  frame push and pop are a small part of an iteration's cost. Every
  variant tried moved register allocation in `step_n`'s hot loop and made
  short loops dearer, so every real file got slower. Don't redo it
  without a new idea for the iteration path itself; see
  `notes/875-loop-frames-in-place.md`. Its loop pins in
  `tests/dispatch_semantics.rs` (`exit`/`stop`/error unwinding, step counts,
  the `execstackoverflow` depth through loop bodies, profiles, a body
  changed mid-loop) hold for any later change to the loop frames.
- **Every `step_n` slice is bounded, empty loop bodies included** (#900).
  An iteration of an empty body yields nothing, so `next_item` runs a row
  of them inside one step; after `EMPTY_RUN` (1024) it returns `Item::Proc`
  with the loop frame on top, which both step loops take as a step that did
  nothing. Before this, `{ } loop` with no limits never left `next_item`,
  freezing the window and `--spool` (only `tick`'s limit poll could stop
  it). Any new path that loops inside `next_item` without yielding needs
  the same bound; see `notes/900-empty-loop-slices.md`.
- Frames that hold external state (glyph contexts' graphics-state
  snapshot, paint suppression) must clean up in **three** unwind
  paths: normal pop, `do_stop`, and `unwind_all`. `ShowCtx::cleanup`
  is the pattern; `Frame::Image` deliberately holds nothing.
- `exit` must not cross Scanner/StopMark/Show/Image/PostOp frames
  (`invalidexit`) — extend that match if you add frame kinds.
- The systemdict self-reference is an Rc cycle (systemdict holds strong
  references to itself and to `userdict`, per the PLRM idiom), and the
  crate has no cycle collector. `impl Drop for Interp` empties systemdict
  and userdict (contents taken out first, freed after the borrow is
  released), so dropping an `Interp` frees its dictionaries and everything
  a program stored in them (issue #625; before that the CLI's
  sweep/spool/selftest loops had to call a hidden
  `break_permanent_dict_cycle(self)`, which is gone). Proven by
  `dropping_an_interp_frees_its_dictionaries` (`Weak` handles to
  systemdict, userdict, `$error`, `errordict`, `FontDirectory`, user dicts
  after a program that uses `save`, `stopped` and `findfont`, and
  `userdict /me userdict put`). Limits: a program that builds its own cycle
  in a plain dict (`a /me a put`) still leaks that dict; an `Object` for
  systemdict that a caller popped out of the interpreter is emptied by the
  drop, since it is the same dictionary. A new long-lived holder of a dict
  `Rc` (a cache on `Interp`) must be a plain field so it drops with the
  `Interp`; a `thread_local` or `static` holding one would make the test
  fail. The `pscat` CLI's single-run path calls the hidden
  `Interp::leave_dicts_to_process_exit` so exit does not walk the graph
  (80 ms on a 600k-entry program); the sweep and spool loops do not.
- A deep dict chain frees iteratively: `impl Drop for Dict` (`object.rs`,
  next to `PsArray`'s) moves nested uniquely held dicts and arrays to a
  worklist, so a 100000-level chain neither overflows the stack at `Interp`
  drop nor on a mid-run `undef`/`restore` (#631). The guard is on `Dict`,
  never on `Object` (see the `PsArray` note above). It costs about 10% on a
  loop that only creates and frees small dicts.
- `build.rs`'s `% @...` tag scanner models the depth-0 operand stack
  with a **two-slot window**, so it only understands `def` whose key is
  exactly two objects back. `/name systemdict /other known def` is read
  as defining `/other` (issue #47 hit this; same family as issue #104's
  open parser gaps). Write such a definition probe-first —
  `systemdict /other known /name exch def`, all on one line so the tag
  block stays directly above the `def` — or teach the scanner operator
  arities, which is a real change, not a tweak.
- **`lib/paintkit.ps` presets each own a scratch prefix, and the
  reason is not tidiness.** `pkribbon`'s body freely redefines every
  `pk*` name on each call, so any preset that calls it (`pkdry`,
  `pkspray`, `pkoil`) must keep its own state under a distinct prefix
  or have it silently clobbered mid-stroke — hence `pb-`/`pz-`/`po-`,
  and `pt-` for `pktrowel` (issue #111), which is the one preset that
  calls `pkribbon` not at all. Pick a fresh prefix for anything new;
  the file header keeps the list.
- **paintkit's brush motions (issue #184) are path builders that
  *replace* the current path, and consecutive `moveto`s collapse.**
  `pkcrisscross`/`pkscumblein` (region operand) and `pkscumble`/`pktap`/
  `pkpull`/`pkzigzag` (the current path is the guide) all start with
  `newpath`, because `scpath` leaves the region's outline as the path and
  a brush would paint it. A bare point per tap does *not* survive: a
  `moveto` straight after a `moveto` replaces it, in pscat and in
  Ghostscript, so a run of point subpaths collapses to the last one. That
  is why `pktap /Length 0` emits `x y moveto x y lineto` (a zero-length
  stroke, which `walkpath` reports as one stop with atend 3, so `pkdab`
  still reads a clump). Scratch prefix `mh-` for the helpers and one
  letter per motion (`mx` `mc` `ms` `mt` `mp` `mz`); none calls a preset,
  so none is clobbered by `pkribbon`.
- **`pkscumblein /Light` filters, it does not re-scatter** (issue #345). A
  lit pass lays the same grid and the same four draws per cell as the unlit
  call, then keeps a circle only when a step of `/Reach` x depth toward the
  light leaves the region (one `scin`), the step shortened by up to
  `/Feather` on the cell's own phase draw. So `/Count` still counts circles
  over the *whole* region (a lit band keeps a fraction: ask for more), the
  lit circles are a subset of the unlit ones under the same seed, and unset
  it takes no extra draw (byte-identical). The depth is the bounding box's
  chord through its centre along the light, not its projection, which
  nearly doubled the band on a wide mass lit at 45 degrees.
- **A touched mark is clipped in canvas space; set the canvas first** (issue
  #182, slices S1-S4; design in `docs/dev/design/CANVAS_SURFACE.md`). `opts pkcanvas`
  records the CTM in force when it is called as *canvas space* and builds a
  seeded weave of knuckles there; `/Touch` below 1 on `pktrowel`, `pkdry`,
  `pkfan`, `pkbroad` or `pkliner` clips the mark to the knuckles its pressure
  reaches (`pjcatch`, called right after the preset's drawing `gsave`). Call
  `pkcanvas` once, first, in the space the piece is painted in, *before* any
  `translate`, `scale` or `rotate` a panel applies: the catch is then built
  under the saved matrix, so every touched stroke, in any later CTM and
  every `pkwet` pass, catches on the same knuckles. A canvas set up inside a
  moved space is a different, moved canvas, and a second `pkcanvas` call
  replaces the first. `/Touch` with no surface raises `pkX-touch-needs-a-canvas`
  (never a silent full mark), even for `/Touch 1`, which renders exactly as
  unset but still needs the canvas. A caller's own
  clip intersects with the catch. `/Touch` takes no `rand` draws, so turning it
  on never moves anything painted after it. The reach box a preset passes to
  `pjcatch` must cover every emitter it has (`touch_box_contains_the_whole_mark`
  is the test); a new adopter writes the same breakdown beside its code.
  `examples/paintkit_canvas_demo.ps` is the specimen (render at
  `docs/dev/design/paintkit_canvas_demo.png`), and `the_weave_is_in_canvas_points_at_any_dpi`
  pins that the pattern is the same at 72 and 144 dpi. To supersample a piece
  with `2 2 scale`, put it *before* `pkcanvas` so the weave scales with it.
  The knuckles wander along their threads and vary in size (#298,
  `pjwander`), so the catch reads as woven canvas, not a pixel grid;
  drykit's `dkcatch` still uses the ruled lattice (#415).
- **A paintkit key means one thing in every preset** (issue #141).
  `/Spread` is always a 0..1 fraction of `/Width`; `/Load` is always a
  contact rate per `/Width` of travel. A new preset that needs a
  different quantity gets a different name (`/Offset`, `/Cover`,
  `/Scatter`, `/Lean`, `/Charge` are the precedents). Every `@param`
  description starts with a bracketed unit, and
  `tests/capabilities.rs::every_migrated_parameter_states_its_unit` fails a
  new parameter that has none in *any* tag-migrated library (#195), from a
  closed vocabulary (`unit_is_known`). The one deliberate exception is
  fillkit's `/Spread`, `[fraction of the box]`, GIMP's name for a sample
  jitter; the test pins it.
- **Abutting filled polygons leave an anti-aliased hairline seam.**
  Two shapes sharing an exact edge do not composite to a solid mass —
  `pktrowel`'s blade lanes had to be given an 18% overlap or a
  supposedly gapless blade rendered as a striped rectangle. Anything
  that tiles fills to make one surface needs deliberate overlap, and
  no unit test will tell you: this was found by looking at the PNG.
- **Don't reach for `setalpha` to make paintkit marks interact with
  what's beneath them.** Alpha applies to *every* fill the mark makes,
  and several presets are built from deliberately overlapping fills
  (`pkoil`'s ridges over its own base; `pktrowel`'s 18% lane overlap,
  directly above). Under a translucent pass those internal overlaps
  composite against *themselves* and return as blotches and a dark
  stripe at every lane boundary — the striped-rectangle artifact again,
  from the other direction. `pkwet` (issue #113) grades *opaque* passes
  toward a caller-declared backdrop instead, which also spares it
  `pkwash`'s Ghostscript fallback. Both were built and rendered before
  choosing; docs/archive/NOTES.md's #113 entry has the comparison.
- **Reading color back off the canvas is not possible in PostScript
  here.** There is no pixel-sample operator (issue #134 is the deferred
  work), so anything that wants to know what is underneath must be
  *told* — `pkwet`'s `/Under` is the pattern to copy, not a stopgap.
  Directional blends don't need readback either: `pkwet`'s `/Direction`
  (issue #181) pulls, lifts and sweeps by re-running the mark along a
  line. Its wrapped proc must paint in the color it *inherits*, or in
  loaded colours (`/Colors`), which `pkwet` grades for you — a proc that
  sets its own color with `setrgbcolor` turns every graded pass into a
  hard echo — and the direction is in the caller's user space, so a reflection's
  mirror goes inside the proc. To pull one colour out of a multi-colour
  self-seeded scene without re-rolling its marks, log `rrand` at each
  painting step and restore it with `srand` on replay
  (`gallery/alpine_lake.ps`'s `lkdo`, issue #191).
  An element that paints only in loaded colours and the inherited colour
  needs none of that: `gallery/evening_stream.ps` (#187) pulls its whole
  far wood into the stream in one directional pull. Don't mirror an element
  that already holds a directional `pkwet` (its 64x nesting budget).
  A directional pull over a long `/Offset` that must fade as a smooth
  gradient wants `/Layers` about `/Offset` + 1 one-sided, 2 x `/Offset` + 1 two-sided (1pt steps; issue #417): the
  explicit cap is 32, the default depth is capped at 16 and has not moved.
  A pull's side edges staircase because each pass's across-wobble is an independent
  draw; `/Smooth` (0..1, default 0, byte-identical when unset; issue #484) low-passes
  the draws per side along the passes, still one draw per pass. It is opt-in: a default
  would move alpine_lake, evening_stream and every other committed pull.
- **Loaded colours: `pkbroad`, `pktrowel`, `pkfan` and `pkdab` adopt them, the rest do
  not yet** (issue #183, slices L1-L5b; design in `docs/dev/design/PAINT_LOADING.md`).
  `lib/paintkit.ps`'s `pu-` section (`puparse`, `puat`, `puflips`, `puwet`,
  `puspell`, `pulightneedscolors`) turns a head position into a colour.
  A preset keeps the record `puparse` returns under *its own* prefix
  (`pmColorRec`, `ptColorRec`), since nested `pkribbon` calls clobber `pk*`
  names and `pu` names are scratch. It is `null` for an unset `/Colors`, and
  every adopting branch tests `rec null ne`, so an unset key never reaches
  new code. `/Light` is left to the preset (`pkdab` owns one): the adopters
  call `pulightneedscolors`, and `puflips` builds the per-subpath flip array
  from the stops' `atend` bits, with no random draws. The colour is set
  *inside the fill* (`pmemit`/`pmstamp`/`ptemit`, behind `ptk 0 gt`), from
  the head position (`pmnc`, `ptnc`, or +-1 for pktrowel's ridges) and the
  flip of the run's first stop, then passed through `puwet`, which grades it
  by the running `pkwet` pass (L3): frames have 10 slots, and 4-6 are the
  per-pass factors, a copy of `/Under` and the running pass index (slot 7,
  issue #179, holds one backdrop per pass for a procedural `/Under` and is
  null otherwise: `puwet` reads it first, so a loaded colour grades toward
  the pass's own backdrop; slot 8, issue #359, is a banded call's record:
  the cells' clips, backdrops, pass colours and the saved random stream, null
  otherwise; the pass loop tests it, and each cell points slot 5 at its own
  backdrop, so `puwet` needs nothing new; slot 9, issue #403, is a `/PerFill`
  call's `[ /Under procedure, CTM at entry, /FillLength or 0 ]`, null otherwise; the third element, issue #492, is read by `pufilllen`). A new
  adopter must call `puwetat` (`puwet` plus the fill's x y) on every loaded
  colour, or its marks go solid inside a pull. `puwetat` is what lets
  `/PerFill true` ask the backdrop at each fill: the four adopters pass the
  run's middle stop (`pkdab`: the dab's centre), and the point is taken to device space and back through
  the slot-9 CTM, so it is in the user space `pkwet` was called in, pass
  displacement included. It never touches the depth counter (an error leaves
  through the pass's own `stopped`), unlike `pqaskunder`, which runs before the
  passes. The ridge and mix colours of `pkoil` and `pkdab` still use `puwet` (per pass).
  Evening Stream's cloud bodies (`evcwet`, #491) are the one gallery adopter: `/PerFill true` with
  `/Under { evskyat }`, no `/FillLength` (their strokes are short circles); the lit passes stay on one sky colour.
  `/FillLength` (#492) cuts one long run into several fills: `ptemit` (pktrowel)
  and `pmemit` (pkbroad) ask `pufilllen` (the smallest positive length among the
  levels running a softening pass, 0 otherwise) and, when it is positive and
  `pusplit` returns more than one piece, take `ptsplitfill` / `pmsplitfill`
  instead of the code they always had (a single piece, or no length, is that
  code, so every old render is unchanged). `pmsplitfill` computes the whole
  boundary first, with `pmemit`'s own draws in its own order, then fills
  sub-polygons that overlap by one segment; `ptsplitfill` has no draws to keep.
  `pkfan`'s `pfrun` (#497) asks `pffillcuts` first and, for more than one piece,
  takes `pfsplitrun` instead of `pfwholerun` (the old body, renamed): the run's
  displaced centreline is drawn whole, in `pfwholerun`'s own order (`pkribbon` draws
  nothing without `/Jitter`), then each piece, from the stop before its cut to the
  next cut, goes to `pkribbon` with `/StartCap` `/pointed` only on the first, `/EndCap`
  `/pointed` only on the last, `/flat` between, the whole run's pitch, and a
  `/Pressure` (`pfpress`) that maps the piece's `t` to the run's so the `/Flick`
  taper stays the run's. `pkfan`'s pressed rays and `pkdab` are one fill each: a new
  adopter that fills a strip should do what `ptemit` does. `pkfan` adopts it too (L4, `pfColorRec`): its colour is set
  per *run* (`pfrun`) and per pressed ray (`pfray`) by `pfcolor`, from the
  bristle's `pfub` and the stored `/ColorJitter` draw `pfcj` (taken where it
  always was, so the stream doesn't move), because a bristle's runs span every
  subpath and `/Light` flips per subpath. `pkdab` (L5, #226) takes `/Colors` per dab: `pdColorRec`, resolved once
  per dab into slot 14 of the dab array from the slot-5 draw it already had
  (`puat` at `u = 1 - 2*pdd5`, flip false, then `puwet`), so all three layers
  read one colour and no draw is added. `puparse` is called unconditionally
  (its unset branch is the spelling and `/ColorMix`-needs-`/Colors` guards, and
  never reads `/Light`); `pulightneedscolors` is not, because `/Light` is
  pkdab's own. `/HighlightColor` and `/ShadowColor` (`puhue`, on `pkdab` and
  `pkoil`) are mix targets graded once through `puwet` at the top of the call,
  and `pkoil` keeps them as `poHighlightColor`/`poShadowColor`. L5b (#227) is
  the specimen, `examples/paintkit_loading_demo.ps` (render at
  `docs/dev/design/paintkit_loading_demo.png`); it writes branches as real strokes out from
  the trunk, because `/Light` flips by chord and a pressed fan is never
  flipped. `/Light 150` puts the first colour on the upper face of every
  branch, `ghostscript_accepts_paint_loading` drives every new key on every
  adopter inside a directional `pkwet`, and `L6`/`L7` (`/TipColor`, `pkdry`,
  `pkoil` ridges) are what is left of #183.
- **Pigments and `pigmix` (issue #185) are a section of `lib/paintkit.ps`,
  before the `pu-` resolver, and touch no preset.** `Pigments` is
  `name -> [r g b strength]` (the `dybox` pattern: a `@kind: Dial` dict, with
  `PigmentNames` for tube order). `pigmix` is Kubelka-Munk per r/g/b channel:
  K/S = sum(a_i (1-R)^2 / 2R) / sum(a_i) with a_i = weight x strength, then
  R = 1 + K/S - sqrt((K/S)^2 + 2 K/S). Scratch names all start `pgm` (pagekit
  owns `pg`+word, the presets clobber `pk*`). Entries are checked even at
  weight 0; one live entry is returned as stored; no random draws. The
  presets get pigments through `pigcolor`/`pigcolors`, which produce the
  `[r g b]` arrays `/Colors`, `/HighlightColor` and `/ShadowColor` already
  validate. Since #356 `puparse` and `puhue` also accept a literal pigment name
  (`/Colors [ /sapgreen /cadmiumyellow ]`, `/HighlightColor /titaniumwhite`),
  resolved once by `pupig` straight from `Pigments`; a pigmix spec in a colour
  key raises `<preset>-colors-pigmix-spec-needs-pigcolors` (or
  `<prefix>-pigmix-spec-needs-pigcolors`), an unknown name
  `...-unknown-pigment-see-PigmentNames`. Tests are
  `tests/pigment.rs` plus two `%%SelfTest` blocks; the specimen is
  `examples/paintkit_pigment_demo.ps` (render at
  `docs/dev/design/paintkit_pigment_demo.png`). Retuning a pigment moves the pinned
  mixes in `reference_mixes_are_pinned`: change them on purpose.
- **`pktrowelplane` (issue #180) is a sibling of `pktrowel`, not a mode.** It
  runs `pktrowel` once per row across the current path's area and passes its
  own options dict straight through, so every `pktrowel` key keeps its meaning
  (the plane's own keys are `/Direction`, `/Overlap`, `/RowJitter`,
  `/AngleJitter`; `pktrowel` ignores unknown keys). Scratch names start `tpl`
  because `pktrowel` overwrites every `pt*` name on each row. The extent is
  taken from `flattenpath` + `pathforall` projected on the pull axis, not
  `pathbbox` under a rotated CTM. The edge is the clip (`clip` before the first
  row, inside a gsave that also restores the caller's path). Gotchas: `RowJitter`
  and `AngleJitter` default to values that cannot open a seam between rows (a
  first render had a bare full-height line at 20% of the row spacing); a dead
  lane from `/Scrape` is a bare track no neighbouring row covers unless
  `/Overlap` is above about 0.5; a `/Pressure` is read at 17 points of the
  row inside the area and the rows are spaced for the smallest |value|, in
  `[0.25, 1]` times the nominal band (issue #362, `notes/362-*.md`; the stream
  is restored with `rrand`/`srand` only when the proc moved it). Tests are `tests/pktrowelplane.rs` plus a
  `%%SelfTest` block; the specimen is `examples/paintkit_plane_demo.ps`
  (`docs/dev/design/paintkit_plane_demo.png`).
- **`pkbrushedge` / `pkfeather` (issue #347) brush a filled mass's edge.**
  Their own section ("Brushed edges") sits after the brush motions.
  `pkfeather` is a motion (replaces the path, paints nothing, one `frnd`
  draw per call and four per slot, slots at a sixth of `/Spacing` as in `pkpull`). The out
  side is decided once per subpath, never per stop: for a closepath'd
  subpath, seven probes just left of the walk are tested against artkit's
  `scpath` region of the whole path (nonzero, like `fill`), so holes of
  either winding and mirrored CTMs work; an open subpath faces `/Outward`
  by the summed normal. Signed area was rejected: it puts a hole's strokes
  into the mass. `pkbrushedge` paints the inward cuts first (sparser,
  `/InSpacing` 2, `/Overlap` mirrored) and the outward strokes last; inward
  cuts as dense as the outward strokes rule a pale band just inside the
  edge. A procedural `/Under` costs one `/Brush` call per inward stroke.
  Every gsave goes through `pegsave` so an error mid-call unwinds to the
  caller's level. Scratch prefixes `mf`/`pe`. Tests: the `%%SelfTest` block
  (guards plus a ring wound both ways and a ring under `1 -1 scale`); the
  specimen is `examples/paintkit_brushedge_demo.ps`
  (`docs/dev/design/paintkit_brushedge_demo.png`); cookbook recipe 10 uses it.
  **Overlapping blobs (issue #381):** with two or more closed subpaths a
  stroke is skipped where a pair of points either side of its anchor are both
  in the whole-path region (an edge hidden inside the others; asked again at a
  sixty-fourth of the step so a hairline hole is not mistaken for one), and
  the side vote votes over 28 places instead of 7. A
  single closed subpath never probes, so single outlines render byte-identically.
  **`/Mass` (issue #426)** is the pulled-out colour, in `/Under`'s three forms
  (unset: the current colour, the old code path). Both keys parse through
  `pecolkey` and a procedure is painted stroke by stroke through `pestrokes`
  / `pecolat` (guard names keep the key's prefix); a procedural `/Mass` costs
  one `/Brush` call per outward stroke, as a procedural `/Under` does per
  inward one, so even a constant procedure paints differently from the array.
  **Region lists (issue #456):** `/Under` and `/Mass` also take
  `[ [ region colour ] ... default ]` (mode 3 in `pecolkey`, parsed by
  `peregions`, looked up per stroke by `pereglook` through the same
  `pestrokes` / `pecolat`); a `{path}` region is built once per call inside
  its own gsave. A convenience over a procedure calling `scin` on a region
  built beforehand, which paints the same strokes byte for byte.
  `notes/456-pkbrushedge-region-under.md`.
  `notes/381-brushedge-hidden-edges.md`.
- **The landscape cookbook (issue #186) is `docs/guides/landscape-cookbook.md`;
  its snippets are the tests.** `tests/landscape_cookbook.rs` pulls every
  ```` ```postscript ```` block out of the markdown (first line `% recipe: <name>`),
  runs it in-process on a 360x240 page, and fails on an error, a lint finding, a
  dirty operand stack, a near-blank page, 5 s of CPU or a missing thumbnail
  (`docs/guides/landscape/<name>.png`, 2x). Edit a snippet in the markdown, then
  `COOKBOOK_WRITE_THUMBS=1 cargo test --profile ci --test landscape_cookbook`
  and look at the PNG; the test does not compare pictures. When a tool the
  cookbook names as pending lands (#180, #229, #230, #345, #346, #347),
  update the recipe in its "Tools not yet landed" table; when Evening Stream
  (#187, PR #348) merges, link its still from the cookbook.
- **Paintkit slices prove byte identity with `scripts/identity_check.sh
  BASE_WORKTREE [BIN] --control FILE [--gallery-still]`** (issue #263), not
  by hand. It renders every tracked `examples/`, `gallery/` and
  `selftest/drivers/` file that loads paintkit from both trees with one
  binary (PNG only, 72 dpi, gallery at 144 dpi = the committed stills'
  2x), about a minute. The base worktree must be at the branch's base
  commit. Exit 1 = a page differs or pscat errored; exit 2 = the negative
  control rendered identically, so the pages prove nothing. Write the
  control as a tiny driver using a key only the branch has.
  A change to the limit paths (`--max-time`/`--max-memory`/`--max-steps`,
  #604) acts only under those flags: pass them with `--pscat-args "--max-steps
  10000000"` (#724), which goes to the base, head and control renders alike.
  Since #303 it fits any lib change that must render identically:
  `--loader lib/<name>.ps` (repeatable, transitive through the libs built on
  it) or `--files LIST` picks the files, and multi-page output
  (`page-001.png`, ...) is compared page by page.
  Two-binary runs (`--base-bin/--head-bin`) also always compare the
  `selftest/drivers/image_*.ps` drivers (header `%%IdentityCorpus: always`,
  #776), the only tracked programs that call `image`, `colorimage` and
  `imagemask`, so an image-code change is checked whatever `--loader` was.
  **An interpreter (`src/`) change uses `--base-bin PATH --head-bin PATH`**
  (issue #335) instead of the shared binary: the base tree renders with the
  base binary (`scripts/build_base_bin.sh BASE_WORKTREE OUT`), the head tree
  with the head one, and the control must use behaviour only the head binary
  has. `scripts/bench_compare.sh` is the matching CPU-time guard.
  A file that fails identically in both trees (same nonzero exit, same log,
  same canvas) is reported `SKIPPED`, not `ERROR` (issue #504:
  `tw_proto.ps` needs an untracked generated `twglyphs.ps`), with a closing
  WARNING; a failure that differs in any way is still an ERROR, and a run
  that skips every file exits 1. A whole-corpus `--files` list of every
  tracked `.ps` therefore exits 0 on an unchanged tree.
- **A "collect stops, then draw runs" brush must not call the caller's
  `/Pressure` proc inside its per-lane/per-bristle loop.** A caller's
  proc is arbitrary code and may consume randomness (`{ frnd }` is a
  legal pressure profile), so a call count that scales with lanes
  couples texture to the pressure profile and breaks same-seed-same-
  picture. Resolve it once per stop into the collected record
  (`pktrowel`'s slot 6). The same discipline applies to every other
  draw in such a loop: keep the *number* of random draws fixed by the
  loop bounds alone, taking them whether or not they're used, or
  turning one artistic control silently re-rolls another's texture.
- **Ghostscript closes sub-2-pixel gaps in fills and clips** (its fill
  adjust). A fine texture built as a clip of many small shapes (a `pjcatch`
  at a small `/Weave`) renders solid in gs while pscat shows it. Guard
  `/.setfilladjust2 where { pop 0 0 .setfilladjust2 } if` inside the
  mark's `gsave` (`docs/dev/design/DRYMEDIA.md` 2.4). `-dGraphicsAlphaBits` doesn't
  help. `dkcatch` does this itself, just before its `clip`.
- **Dict `forall` order is stable, but it is not insertion order for
  names.** Non-name keys (integers, reals, booleans, array and dict keys)
  come back in insertion order (#277: each exotic entry carries a sequence
  number and `Dict::pairs` sorts by it; redefining keeps the slot,
  `undef` then `put` moves it to the end). Name keys come first, in the
  interned-id hash order: deterministic for a given program but not
  insertion order, because name lookups are the hot path and carry no
  sequence number. Before #277 the non-name half used a randomly seeded
  hash map and changed on every run (#258's symptom). `dkcatch` still
  keeps its own first-seen list (it predates the fix).
- tiny-skia `Transform::from_row(sx, ky, kx, sy, tx, ty)` matches the
  PS matrix order `[a b c d tx ty]` — `ops/matrix.rs` documents it;
  don't rediscover this the hard way.
- **artkit, paintkit, drykit, drychalk and drycharcoal define every procedure
  `} bind def`** (#307, #318, #326), and `tests/libbind.rs` fails for any procedure added without
  it. Ghostscript resolves each executable name in a procedure body at
  run time, and with several libraries loaded those lookups are what
  make gs slow (alpine_lake 30 s unbound, 10 s bound). A private scratch
  dictionary (the drychalk pattern) was tried for drykit and paintkit and
  is *slower*: a non-userdict dict on top of the dict stack costs every
  operator lookup about 30% more (`notes/307-*.md` has the numbers). The
  catch is that an operator redefined after a library loads no longer
  reaches its procedures; library names that are not operators are still
  late-bound, so overriding `dkbudget`, `pkribbon` and the like works.
  drychalk's `dcscratch` private dictionary still pays once the library is
  bound (20% to 30% on its specimen), so it stays. drycrayon and the older
  kits are deliberately unbound: drycrayon gained under 1%, and the rest
  take 0.33 s or less in gs or cannot run there (no fonts), so there is
  nothing to win (`notes/318-bind-drychalk.md`). drycharcoal is bound too, but
  it gains little: 0.4% of the instructions on its specimen and, in one
  pair of runs, 3% on the Lincoln portrait in gs (`notes/326-bind-drycharcoal.md`).
- Test Interps at 100×100; golden runs at 500×500/r72. Metric
  assertions use 1e-3 tolerances where the pen is f32.
- **Program output goes through `Interp::write_stdout`** (#252), not
  `println!`. `=`, `==`, `stack`, `pstack` and `print` call it; the wasm
  build turns on `capture_stdout` (a wasm32 module has no stdout) and
  `pscat.js` exposes the buffer as `ps.output`, which the playground
  shows. A new printing operator must use it too, or its output vanishes
  in the browser.
- **Printed stack and operand output is bounded (#703, #745)**: one writer,
  `Object::write_repr` in src/object.rs, spells every `==`-syntax value, so
  `repr()` (unbounded but for depth 32, `UNBOUNDED_LIMITS`), `==` and
  `pstack` (`OPERATOR_LIMITS`: depth 32, 1024 elements, 4096 bytes per
  string/name, 16 KiB per operand; `OPERATOR_STACK_BYTES` 32 KiB across the
  stack, then one `... truncated` line) and the `--pstack-on-error` dump
  (`stack_repr_bounded`, `STACK_DUMP_LIMITS`) cannot drift. `stack` cuts
  strings via `text_bounded`. `=` and `print` are deliberately not bounded
  (a program's own text output). The undefined/offending name in an error
  line is cut to `ERROR_NAME_LIMIT` (512) by `cap_text`. `cvs` and
  `format_real` digits are untouched. Notes: `notes/745-bound-operator-output.md`.
- **The npm package (`npm/`, #566) is that wasm build plus a JS API.**
  `scripts/build_npm.sh` runs `build_wasm.sh` and copies `web/pscat.js` in
  as `core.js`, so a change to the C ABI (`src/wasm.rs`) or to
  `web/pscat.js` ships in the package too: keep `npm/renderer.js` and
  `npm/index.d.ts` in step, and run the `npm` workflow's steps
  (`build_npm.sh --pack`, `npm test` in `npm/`, `cargo test --test wasm`,
  `test_npm_tarball.sh`). The standard 35 are compiled in (#757: the
  Liberation core plus `fonts/base35/`, `font::EMBEDDED`); fonts beyond
  them reach wasm only through `ps_add_font` (`font::register_font_bytes`),
  keyed by font pack file stem like the disk catalog. A pack face not added
  that way gets the same stand-in as native (#728) and its note, worded for
  `addFont`, goes to `Interp::font_warnings` (no stderr in wasm):
  `ps_warnings_*` in `src/wasm.rs`, `Renderer.warnings` in npm (#750);
  `PostScriptError.warnings` and `web/index.html` show them too (#804).
  `npm/package.json` moves in lockstep with
  `Cargo.toml` (`release_stages.py bump-manifests`; a test fails if they
  differ), and the release publishes the tarball `verify` packed
  (docs/dev/RELEASING.md). Something new embedded in the wasm shows up as
  a size-budget failure in `build_npm.sh`.
- **The standard 35 are all compiled in (#757), but not all in
  `BUILTINS`.** The twelve Liberation faces are builtins (fixed FIDs,
  StandardEncoding, `FontName` = the PostScript name). The other 23
  (`fonts/base35/`, `font::EMBEDDED`) register as catalog faces on first
  use, after the disk lookup, so they keep the encodings and `FontName`
  the `texgyre`/`extras` packs gave them; routing them through
  `BUILTINS` would break Symbol's encoding and every render that names
  them. `fonts/base35/` is in the crate's `include`; `fonts/catalog/` must
  never be (`scripts/check_licences.py package`).
- **The Learn page's examples are files** (#252): `site/learn/NN-name.ps`,
  plus `NN-name.out` with exactly what it prints. `site/learn.html` names
  them with `<!--@example NAME "alt text"-->` markers that
  `scripts/site_examples.py` expands at build time; `tests/learn.rs`
  checks each against its `.out` in pscat and in gs, and
  `scripts/check_learn_examples.mjs` (run by `build_site.sh`) in the wasm
  build. Change an example's output and you must change its `.out` and
  the prose around it. `playground.html?example=learn/NAME.ps` is how
  the "Try it" links open them.
- **The front page's `pscat chalk` boards are a list plus a build step**
  (#849). `site/chalk/boards.json` holds each board's id, LaTeX and extra
  flags; `scripts/site_chalk.py render` draws them with the release binary
  into `_site/assets/chalk/` (the exact argv the page prints), and `expand`
  fills `<!--@chalk-showcase-->` in `index.html` with a figure and the
  `shlex`-quoted command per board, before `site_pages.py` runs. No PNG is
  committed, so the boards re-render with every renderer change. The
  README's picture is `assets/chalk/quadratic.png` on the Pages site (an
  absolute URL, as crates.io needs; the crate's `include` has no images).
  Add a board by adding an entry; `tests/site.rs` checks each is drawn.
- **`tests/site.rs` builds into its own scratch dir** (#767). `build_site.sh`
  wipes and rebuilds its output directory, so two runs in one worktree (the
  gate beside a hand-run `cargo test`) used to delete each other's `_site/`
  and fail with "examples no page shows". The test now sets
  `PSCAT_SITE_OUT` (a `common::scratch_dir`) and `PSCAT_BIN` (its own
  `CARGO_BIN_EXE_pscat`, so the script skips `cargo build --release`); both
  default to the old behaviour for CI and the Pages workflow. Run
  `build_site.sh` by hand with the defaults and it still writes `_site/`.
- **`screct`/`scpath` stamp their region, and `scin` trusts the stamp**
  (#503, `notes/503-artkit-scin-region-stamp.md`). `/Checked` holds the very
  array in the region's `/Edges` (path) or `/BBox` (rect); `scin` skips its
  whole-region check (`scregionvet`) when `eq` says so, and checks a
  hand-built or copied-and-edited region on every call. Don't make the stamp
  point back at the region dict: pscat dicts are `Rc`, a self-referencing
  one is never freed, and 20000 `scpath` regions cost 220 MB. A built
  region's array edited in place after the build keeps the stamp
  (`region /Checked undef` re-arms it). `siinside` has no check; paintkit's
  direct callers are #526.

## Verification checklist before calling any future stage done

```sh
cargo test --profile ci  # all suites, golden included (needs gs installed);
                         # optimized + debug-assertions, what CI runs (#204)
cargo clippy --all-targets
cargo fmt --check
./scripts/selftest.sh                     # PS-native checks (no gs, no Rust)
cargo run -- examples/postcard.ps         # watch it draw
./gallery/show.sh --live                  # the fun regression suite
```
