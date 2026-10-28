# Chromatic Bloom demonstration

A psychedelic concert poster built from repeated flower geometry, PaintKit
stems, an orbital ground, seeded flecks and standard Helvetica/Courier type.
There are three colourways: Ultraviolet, Sunprint, Blue Hour. All pictures in
[index.html](index.html) are actual local PSCat renders, not substitute images.

## Sources and data flow

| File | Role |
|---|---|
| [common.ps](../../examples/functional/common.ps) | Shared palettes, site records, petal, flower, stem, ground and type. Loads artkit + PaintKit. |
| [poster.ps](../../examples/functional/poster.ps) | Functional record pipeline and configured drawing score. Loads functional + aspect first. |
| [imperative.ps](../../examples/functional/imperative.ps) | Equivalent manual collection/index/counter/trace orchestration. No functional/aspect dependency. |
| [primitives.ps](../../examples/functional/primitives.ps) | Same flower quotation with three captured palette values on one sheet. |
| [detail.ps](../../examples/functional/detail.ps) | Large rendering of the shared flower. |
| [benchmark.ps](../../examples/functional/benchmark.ps) | Numeric map/fold control, no art rendering. |
| [functional_demo.py](../../scripts/functional_demo.py) | Render all artifacts and run paired CPU measurements. |

The index array contains 0 through 24. `df-site` maps indices 0..23 onto an
ellipse, with radius/angle/colour-index derived deterministically from index.
Index 24 is the central flower. A record is `[x y radius angle colour-index]`.
`df-visible` selects colour-index zero, so 12 peripheral flowers plus the
central flower remain. `df-weight` adds radii: the selected sum is 622.
Geometry needs no randomness. Ground flecks use the explicitly reset seed
314159; all three colourways use the same draw order and RNG consumption.

The chosen palette configures two quotations with curry:

```text
stem       ( record -- )
blossom    ( record -- ), decorated with visits aop-count
{dup} + stem + blossom => draw ( record -- )
flowers + draw => fp-each
```

`{dup}` then stem leaves the preserved record; blossom consumes it. The whole
score is traced as `(garden)`. An iteration therefore draws one stem and one
flower and increments visits once. The artistic primitives own their local
dictionaries and balanced graphics saves; functional callbacks do not add a
hidden dictionary scope. Native fixed-count loops rotate petals; using a
combinator for every primitive operation would not improve this example.

## Reproduce

Run from the repository root with this feature's code, not a pre-feature
published binary:

```sh
cargo build --profile ci --locked
python3 scripts/functional_demo.py render
# The script builds the ci binary itself if --pscat is not supplied.
# Optional explicit existing binary:
python3 scripts/functional_demo.py render --pscat target/ci/pscat
```

Direct commands:

```sh
target/ci/pscat --page 720x960 --lint-strict --png bloom.png \
  examples/functional/poster.ps
target/ci/pscat --page 720x960 --lint-strict --png sunprint.png \
  -e '/Style 1 def (examples/functional/poster.ps) run'
target/ci/pscat --page 720x960 --lint-strict --png blue-hour.png \
  -e '/Style 2 def (examples/functional/poster.ps) run'
target/ci/pscat --page 720x960 --lint-strict --png control.png \
  examples/functional/imperative.ps
target/ci/pscat --page 720x300 --png primitives.png \
  examples/functional/primitives.ps
```

For a windowed live rendering, omit `--png`/`--headless` from the poster command.
A ground-only intermediate uses `/Layer 1 def`; `/Layer 2 def` draws flowers
without type; the default Layer 3 draws the complete poster. Ground-only output
has zero counted flowers but the same precomputed checksum 622. `Style` must
be an integer in 0..2; `Seed` is an ordinary PS random seed. The demo is not a
general untrusted-options API; Layer and Seed follow normal operator checks.

Default stdout (before the CLI's optional “wrote PNG” line):

```text
enter garden
leave garden
flowers=13
radius-checksum=622
```

## Outputs

[assets/](assets/) contains `ultraviolet.png`, `sunprint.png`, `blue-hour.png`,
`imperative.png` (default control), `ground.png`, `primitives.png`, `detail.png`
plus the four typography/AOP images documented in
[WASM-STUDIO.md](WASM-STUDIO.md), and [render-manifest.json](assets/render-manifest.json) with sizes and SHA-256
hashes. Full posters are 720×960, vocabulary 720×300, detail 480×480.

The offline HTML lets a reader select the three pre-rendered colourways and
switch between default ground and finished render. It shows the exact command
for the selected image. It does not execute PS or synthesize new artwork in JS.
The ground is always Ultraviolet, and the UI says so when selected.

## Comparison and modification

The imperative control shares drawing helpers to isolate orchestration; it
manages output slots, selection count, checksum, trace and count explicitly.
It can reuse the same helpers and produce equally good artwork. The drivers
have 19 vs 24 non-comment/nonblank lines, excluding shared helpers and the new
libraries. This small count is descriptive, not evidence of general maintainability.
The Rust demo test asserts all three full pixel buffers match and compares
against committed PNGs. [Testing](TESTING.md) describes CPU observations.

Change `df-palettes` to alter colour without editing marks/layout. Change
`df-site` for placement or `df-visible` for selection; update expected count,
checksum and golden outputs deliberately after inspecting them. Replace the
configured stem/blossom quotations to try another existing PaintKit mark.
Callbacks must retain `(record -- )` and balanced graphics/dictionary state.
Do not hide geometry/style changes inside AOP; reserve advice for observation
and contracts. Keep shared primitives shared if measuring orchestration.


## Further studies: live type and observable execution

The [live WASM studio](live-studio.html) adds two more demanding examples:
measured, contoured letterforms in two colour fields, and an AOP proof with
before contracts, normal-return counters, around diagnostics and a real failure
probe. Both run from local files without a server, can be edited through bounded
text controls, and are rendered by the existing WASM interpreter. Their shared
source, output, limits and rebuild commands are in [WASM-STUDIO](WASM-STUDIO.md).
They do not change the original poster's benchmark workload or measurements.
