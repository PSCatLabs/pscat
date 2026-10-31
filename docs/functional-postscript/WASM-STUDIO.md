# Live typography and AOP studio

Open [live-studio.html](live-studio.html) directly in a modern browser. This page
executes **the existing PSCat WebAssembly interpreter** locally. It does not use
JavaScript to approximate PostScript drawing and does not merely swap PNGs.
The original [introduction](index.html) still includes saved, inspectable output.

## Two new studies

### Type becomes terrain

[Source](../../examples/functional/type-terrain.ps), shared
[glyph vocabulary](../../examples/functional/type-common.ps).

Each word is mapped to character strings, measured advances and ink bounds
using the selected bundled outline face at a reference size. An extent fold
combines advances, sidebearings, overhangs and vertical bounds. The run scales
to fit **both** width and height and is centered when height is limiting. This
keeps short words such as I or J from becoming oversized. The terrain cap is
210 user units; the proof cap is 170 so its labels stay clear.

A layout fold returns fresh cursor/index tuples and writes records into a fresh
private output array: `[character x baseline size index font-name]`. The font is
captured in each record; changing TypeFont later cannot silently change its
outlines, metrics or diagnostic bounds. No incoming text/records are mutated.
This is controlled builder mutation, not deep functional immutability or
kerning-aware shaping.

`fp-curry` captures a treatment dictionary into `ty-letter`; nested `fp-each`
visits runs and glyphs. `charpath` supplies real outline geometry.
`aop-count` observes actual glyph calls; the default WORDS / MAKE / WORLDS score
still executes 15 glyphs.

The shared **Typeface** selector applies to both studies:

- Helvetica-Bold and Helvetica-BoldOblique
- Times-Bold and Times-BoldItalic
- Courier-Bold
- Palatino-Bold
- Bookman-Demi
- AvantGarde-Demi

All eight are in PSCat's bundled standard 35: no font downloads or packs. Native
configuration is `/TypeFont /Times-BoldItalic def`, for example. The common
vocabulary rejects faces outside this list rather than relying on substitution.

Six independent **Print treatment** choices are available in terrain:

| TypeStyle | Treatment | Drawing behavior |
|---|---|---|
| 0 | Nocturne | Solar colour contours, midnight ground and extrusion. |
| 1 | Sunprint | Tidal contours, warm paper and a red shadow. |
| 2 | Blueprint | Fine engraved grid inside blue glyphs, pale contours. |
| 3 | Risograph | Coral/gold colour, paper-coloured dot screening, teal shadow. |
| 4 | Neon | Layered cyan, pink and cream outline strokes on purple. |
| 5 | Op Art | Black/ivory optical waves with a pale dimensional shadow. |

Font and treatment selection automatically rerenders the actual PS; all 48
combinations are exercised under WASM. These are deterministic vector marks,
not raster textures, imported images or approximated JavaScript fonts. Preset
PNG examples pair some treatments with different faces to show the range;
choosing a treatment in the studio does not change the independently selected
font. The typography's captions use fixed interface faces for consistency.

Text controls remain one to ten A–Z letters per line. PS strings here are bytes,
not Unicode graphemes. Layout uses advances and ink bounds but does not implement
kerning, ligatures or general shaping. Existing font/charpath semantics are
unchanged. Use Render now or Watch it draw after editing words.

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

The render script writes the six `type-*.png` treatments and two `press-*.png`
proof views under [assets/](assets/), along with the original garden artifacts.
All fifteen saved renderings are covered by the hash manifest. For example:

```sh
target/ci/pscat --page 960x1200 --lint-strict --png neon.png \
  -e '/TypeStyle 4 def /TypeFont /Times-BoldItalic def (examples/functional/type-terrain.ps) run'
```

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
checker verifies decompression, the WASM header, the embedded API's recorded
hash, and exact inlining of the current example sources. It pins hashes only
for inputs this feature owns (`examples/functional/`, `lib/functional.ps`,
`lib/aspect.ps`); engine, `web/pscat.js` and shared-library hashes are
provenance, because main changes them independently. Rebuild whenever a
feature-owned source changes. To see whether the engine has moved on since the
payload was built:

```sh
python3 scripts/build_functional_studio.py --check
```

A Rust integration test executes the exact payload under Node and compares
full output pixels against the native programs whenever that check passes; it
skips with a note once the payload predates the engine. No WASM compiler is
needed merely to run that test.

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

The Node check executes all 48 font/treatment combinations and all eight AOP
proof fonts without substitutions. Native integration compares complete pixel
buffers and stdout for 15 representative configurations; saved outputs and
same-coordinate target identity have separate checks. A layout regression
checks I, J, Q, WIDE and MWMW for all fonts, including height bounds, negative
sidebearings and captured-font identity after TypeFont changes. Browser checks exercise text/palette changes, inspection/fault
toggles, changed word counts, invalid input, stop/restart, desktop/mobile layout,
PNG export and source display, with no page HTTP(S) requests. See the current
validation record in [PROGRESS](PROGRESS.md). These checks support the included
examples, not general PostScript or all browser implementations.
