# Live typography and AOP studio

Open [live-studio.html](live-studio.html) directly in a modern browser. This page
executes **the existing PSCat WebAssembly interpreter** locally. It does not use
JavaScript to approximate PostScript drawing and does not merely swap PNGs.
The original [introduction](index.html) still includes saved, inspectable output.

## Two new studies

### Type becomes terrain

[Source](../../examples/functional/type-terrain.ps), shared
[glyph vocabulary](../../examples/functional/type-common.ps).

Each word is mapped to one-byte character strings and measured advances using
bundled Helvetica-Bold at a reference size. Fold sums advances to fit a fixed
width. A second fold returns fresh cursor/index tuples and writes records to a
fresh, private output array: `[character x baseline size index]`. No source text
or incoming records are mutated. This is controlled builder mutation, not a
claim of deep functional immutability or kerning-aware shaping.

`fp-curry` captures a style dictionary into `ty-letter`; nested `fp-each` visits
the three runs and their glyphs. `charpath` supplies actual outline geometry.
The outline clips a multistop gradient and vector contour lines. Repeated
translated glyphs supply the extrusion. One palette value changes the whole
colour treatment. `aop-count` observes actual glyph calls; a trace wraps the
score. Default WORDS / MAKE / WORLDS executes 15 glyphs.

Controls rerender text and palette, or step the same program visibly. The
interactive input is intentionally one to ten A–Z letters per line. PostScript
strings here are bytes, not Unicode graphemes. Layout sums character advances;
it does not implement kerning, ligatures or general language shaping. Existing
font and charpath semantics remain unchanged. No external font packs are used.

### The observable press

[Source](../../examples/functional/observable-press.ps).

The first proof row invokes `ty-flat-letter` directly. The second wraps the same
target, in this explicit order:

```text
outer entry counter
  -> around advice
     -> before contract
        -> original target
        -> after normal-return hook
     -> optional ink-bounds / advance diagnostic
```

Before advice verifies a numeric, positive size without consuming the record.
After advice increments `/Completed` only on normal return. Around advice uses
`1 index {exec} fp-dip` to hide a copy of the record on the execution stack,
proceed with exactly the original target stack, then optionally draw diagnostics.
These are diagnostic marks, not hidden artistic transformations. `gsave` around
the overlay preserves the caller's path, font, colour and CTM.

An isolated invalid-size fixture is caught with `stopped`. The catcher checks
the **exact** `obs-font-size-must-be-number` command, discards the one fixture
record left by that contract, and records a rejection. It does not swallow
arbitrary errors or claim to be a generic cleanup utility. The real offending
command is an executable name: `/obs.fault load` retrieves it as data for the
readout, avoiding accidentally reexecuting the guard.

Default SIGNAL has six valid glyphs and one fault probe: **7 attempts / 6 normal
returns / 1 rejection**. Disable the fault: **6 / 6 / 0**. Change the word to
DREAM with the fault enabled: **6 / 5 / 1**. The canvas audit panel and stdout
are populated from actual metric dictionaries after execution. Entry/return
marks show counts, not elapsed timing. There is no invented timer sample.
The return hook and diagnostic overlay do not run for the rejected entry.

The fault option is `/InjectFault`; diagnostics use `/Inspect`; the word is
`/ProofText`. Disabling diagnostics preserves the target pixels. Rendering
without diagnostics does not disable contracts or observation counters.

## Run the native programs

From the repository root:

```sh
cargo build --profile ci --locked
target/ci/pscat --page 960x1200 --lint-strict --png type.png \
  examples/functional/type-terrain.ps
target/ci/pscat --page 960x1200 --lint-strict --png sunprint.png \
  -e '/TypeStyle 1 def (examples/functional/type-terrain.ps) run'
target/ci/pscat --page 1280x960 --lint-strict --png proof.png \
  examples/functional/observable-press.ps
target/ci/pscat --page 1280x960 --lint-strict --png clean.png \
  -e '/Inspect false def (examples/functional/observable-press.ps) run'
python3 scripts/functional_demo.py render
```

The render script now also writes `type-nocturne.png`, `type-sunprint.png`,
`press-inspected.png`, and `press-clean.png` under [assets/](assets/). All eleven
saved renderings are covered by the hash manifest.

## How file:// execution works

The normal `Pscat.load()` fetches a wasm URL. Browsers generally block such
fetches from local files. This studio instead includes a generated **classic
local script** with three payloads:

1. The existing `web/pscat.wasm`, gzip-compressed and base64-encoded.
2. The **unchanged** [web/pscat.js](../../web/pscat.js) API source.
3. Both actual example programs, with their shared `.ps` vocabulary inlined.

A browser-native `DecompressionStream('gzip')` restores the module. A Blob URL
imports the unchanged API, then `Pscat.instantiate(bytes)` instantiates the real
module without fetching anything. Standard `(lib/...) run` resolves against the
libraries already compiled into PSCat. The example include is inlined because
WASM has no checkout filesystem. No new engine operator/runtime/library or
third-party JavaScript dependency was introduced.

The raw module is about 11.5 MB; the generated local JS payload is about 7.1 MB.
It includes the existing bundled fonts and art libraries. This is a deliberate
size cost of a portable, server-free demo. The normal documentation and PNGs do
not require executing this payload. The studio requires modern WebAssembly,
Blob modules and native gzip decompression; tested in Chrome. Unsupported
browsers retain a saved preview and display an error instead of claiming a live
render. Broader browser support is not established by these checks.

[studio.js](studio.js) only constructs bounded preset parameters, steps the
interpreter and paints returned RGBA. Words are validated as ASCII and escaped
as PostScript strings. It is not a general untrusted code editor. Render-now
runs 50,000-object slices; watch mode uses 2,500-object slices and paints each
frame. Stop cancels scheduling, leaves a labeled partial image and disables
PNG download until a complete render. A later render restarts with `begin()`.
Live stdout is `Pscat.output`; no log lines are synthesized by the UI. Browser
`performance.now()` supplies explicitly labeled elapsed time, not CPU time;
PostScript clocks in WASM still read zero.

## Rebuild, provenance and licences

```sh
# Rust wasm32-unknown-unknown target must already be installed.
python3 scripts/build_functional_studio.py
# Or reuse a just-built module:
python3 scripts/build_functional_studio.py --skip-wasm-build
node scripts/check_functional_studio.mjs
python3 scripts/check_functional_docs.py --pscat target/ci/pscat
```

The builder reuses `scripts/build_wasm.sh`; it does not change the WASM API.
The generated payload is committed for immediate offline use. Do not edit it
by hand. [studio-manifest.json](assets/studio-manifest.json) records raw/gzip
module hashes, sizes, original API and example/library source hashes. The docs
checker verifies decompression, the WASM header, source/API identity, and exact
inlining. Rebuild whenever its relevant source changes. A Rust integration test
executes the exact payload under Node and compares full output pixels against
the native programs; no WASM compiler is needed merely to run that test.

The module retains PSCat's existing combined licence expression:
`(MIT OR Apache-2.0) AND OFL-1.1 AND LPPL-1.3c`. See
[project notices](../../THIRD_PARTY_NOTICES.md),
[MIT](../../LICENSE-MIT), [Apache](../../LICENSE-APACHE),
[OFL](../../LICENSE-OFL), [font notices](../../fonts/README.md), and the
[generated WASM dependency licence texts](assets/wasm-dependency-licenses.html).
The precise bundle path is classified in `licenses/assets.json`; this is an
existing PSCat build, not a third-party language implementation.

Dependency notices were generated with the existing tool/configuration:

```sh
cargo about generate --locked --offline --no-default-features \
  --target wasm32-unknown-unknown --fail about.hbs \
  -o docs/functional-postscript/assets/wasm-dependency-licenses.html
```

Normalize only generated line endings/trailing whitespace before committing:

```sh
python3 -c 'from pathlib import Path; p=Path("docs/functional-postscript/assets/wasm-dependency-licenses.html"); p.write_text("\n".join(s.rstrip() for s in p.read_text().splitlines())+"\n")'
```

## Validation scope

The Node check executes actual typography, decorated/failing proof and a changed
word with fault disabled. Native integration compares complete pixel buffers
and stdout. Browser checks exercise text/palette changes, inspection/fault
toggles, changed word counts, invalid input, stop/restart, desktop/mobile layout,
PNG export and source display, with no page HTTP(S) requests. See the current
validation record in [PROGRESS](PROGRESS.md). These checks support the included
examples, not general PostScript or all browser implementations.
