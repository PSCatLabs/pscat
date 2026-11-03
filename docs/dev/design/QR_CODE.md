# Native QR Code library

Status: first-session development on `feature/qr-code-library`, based on
`07f054c19d7045281a667e185b9bfc74f3cde29e` (main, 2026-10-28).
This is a PSCat core library and gallery enhancement. No merge to main is
authorized for this session. The work is not yet a complete QR encoder.

Review package: [review handoff](QR_CODE_REVIEW.md),
[implemented API](QR_CODE_API.md) and [verification/reproduction](QR_CODE_TESTING.md).
The checkpoint is draft PR
[#957](https://github.com/PSCatLabs/pscat/pull/957).

Current implementation in `lib/qr.ps`: byte packing, automatic version/capacity
planning, Reed–Solomon correction and block interleaving, and (#970) function
patterns, zigzag placement and forced masks, and (#971) format/version BCH
metadata, the four penalty rules and automatic mask selection, giving complete
symbols that an independent decoder reads back, with exact named guards,
selftests, independent fixtures and native/Ghostscript/embedded-loader checks,
and (#972) the public `qrencode`/`qrdraw` API with vector drawing, whose PNG,
SVG and PDF exports are rasterized and decoded independently.
The original `qr--byte-data` seam accepts explicit capacity; version selection
now lives in `qr--byte-plan`. No Rust engine changes or runtime dependencies
were added; #971 adds one test-only dev-dependency, the `rqrr` decoder. A separate same-family plan review
confirmed this slice and identified count-width and direct frame-leak tests,
both included. Final cross-family PR review remains outstanding.

Initial checkpoint validation: locked offline CI-profile build and full repository suite passed;
all five new QR tests (including real Ghostscript and embedded loading), two
inline selftest blocks, reference regeneration equality, all-target Clippy with
warnings denied, formatting, links, gallery and license checks passed. The shell
selftest wrapper's redundant Cargo build was denied an external-target lock;
its underlying manifested-library and strict-driver checks passed in the full
`tests/selftest.rs` suite. No rendered-symbol or card acceptance is claimed.

## Implementation sequence

1. Byte-mode data packing, verified against independent frozen codewords.
2. Version/capacity planning and GF(256)/Reed–Solomon block interleaving.
3. Function patterns and zigzag data placement; fixed-mask reference matrices.
4. Eight masks, format/version BCH bits and penalty selection; first independently
   decoded repository URL, before any card decoration.
5. Public interface and vector drawing, embedded-resource and exporter checks.
6. Versions 1–40/ECC boundary sweep, failure/limit diagnostics and decode corpus.
7. Procedural two-sided business card, physical sizing, gallery integration and
   independent visual review.

Each numbered step is independently verifiable and gets a tested commit. See
[issue drafts](QR_CODE_ISSUES.md) for dependencies and acceptance criteria.
The first pipeline target is versions 1–4; the finished library must support
all Model 2 versions 1–40. Do not advertise partial version support as complete.

## Boundaries and provisional public interface

All encoding is ordinary PostScript, initially in `lib/qr.ps`, divided into
data, correction, matrix, mask, drawing and public-interface sections. Split
files only if size/maintenance justifies it; metadata `@requires` does not
execute a load chain. No QR interpreter operator, Rust encoder, package or
service is needed. Rust integration tests may inspect output or invoke tools.

Implemented interface (#972; [contract](QR_CODE_API.md#public-api-qrencode-and-qrdraw),
[guide](../../guides/qr-codes.md)):

```postscript
(lib/qr.ps) run
18 18 moveto
(https://github.com/PSCatLabs/pscat) << /Level /M >> qrencode
<< /Side 90 /Dark [0 0 0] /Light [1 1 1] >> qrdraw
```

Two departures from the first proposal below: `qrdraw` draws at the current
point instead of taking `x y side` operands, with the size in its options
(`/ModuleSize` or `/Side`), and the colors are `/Dark` and `/Light`, the
standard's own terms, rather than `/Foreground`/`/Background`. `/Modules` is
an array of row arrays. The original proposal follows for the record.

`qrencode`: payload string and options -> dictionary with `/Version`, `/Level`,
`/Mask`, `/Size` and `/Modules`. Modules are row-major booleans, top-left origin;
they exclude the quiet zone. Default exact level M; choose the smallest fitting
version. Never silently boost/change ECC. Explicit version/mask options are
useful for reference tests. Fail clearly on excessive input or invalid options.
Strings contain raw bytes: ASCII URLs work directly; no implicit Unicode/ECI
claim. ECI and mode optimization are future additions.

`qrdraw`: symbol, lower-left x/y, square side in user units and options -> no
operands. Side includes the quiet zone (default and minimum four modules).
Paint background then axis-aligned filled squares/runs. Preserve the caller's
path and graphics state. Default dark modules on opaque white; RGB options are
validated, but callers remain responsible for choosing scan-friendly contrast.
No rounded modules, logos or unbounded decorative hooks in the initial API.

The first internal seam is `qr--byte-data`: raw string, explicit version,
data-codeword capacity -> a fresh padded data string. The future planner owns
the standards capacity/ECC tables. This seam alone is not a QR symbol.

## Reuse and repository discovery

- `build.rs` already recursively embeds `lib/**/*.ps`; `src/paths.rs` resolves
  `(lib/...) run` from disk or embedded resources. No loader-table edits needed.
- `% @internal` / public capability tags integrate discovery; `%%SelfTest`
  blocks and `selftest/libraries.txt` cover exact named guard failures.
- Existing `tests/common/mod.rs` supplies unique scratch directories and
  child-process deadlines. Use these for all Rust tool/export tests.
- Existing strings, shifts, xor, arrays and dictionaries suffice for encoding.
- `src/svg.rs`, `src/pdf.rs`, `src/gfx.rs` supply vector/PDF/raster output;
  `tests/pages.rs` and `tests/pdf.rs` are the multipage/physical-unit precedents.
- Main's standard 35 fonts, `charpath`, artkit palette/gradient/text layout and
  `lib/pagekit.ps` are available for the card. Avoid font-pack dependencies.
- `gallery/manifest.json` is authoritative; use `scripts/gallery.py sync/check`,
  never hand-edit generated site gallery pages. Gallery/example art is CC0;
  library/test code is MIT OR Apache-2.0.

Remote branches inspected: main, `feature/functional-postscript`,
`feature/683-plugin-icon-pscat-logo`, `fix/866-plugin-launcher-windows-support`.
Open issues/PRs were inspected; no existing QR issue or branch was found.
The initial filtered clone has shallow history; fetch specific history when
needed rather than downloading all historical art assets.

### Functional branch decision

Read its README, architecture, API/progress and WASM-studio documentation,
examples/test organization and saved browser validation. Its staged commits
separate design, PS libraries, examples and documentation/testing. Its offline
studio runs the actual existing WASM engine with typography/AOP examples;
the checked-in validation covers text/palette changes and real counters.
This session reviewed source and saved browser evidence, not a new live run.

The branch is unmerged, with independent review deliberately outstanding.
QR encoding has no dependency on `functional.ps` or `aspect.ps`. Use main's
ordinary procedures for card styling. If the functional work lands before card
implementation, consider a separately reviewed optional demo adaptation on top
of that main commit. Never copy or silently merge unfinished implementations.

## Verification and risks

Use [Nayuki's independent encoder](https://github.com/nayuki/QR-Code-generator)
as an oracle, pinned to `3c6d0b3cefb4e049dc337e82237c9644399716a8`.
Generate data fixtures by intercepting its correction input; do not recreate
our bit-packing algorithm in the test oracle. Keep normal tests offline and
dependency-free. Later compare fixed-mask matrices (mask-choice ties need not
produce identical automatic masks) and pin diagnostic mismatches by stage.
For print geometry, DENSO WAVE documents the
[four-module quiet zone and module sizing](https://www.qrcode.com/en/howto/code.html/index.html).
Consult ISO/IEC 18004 for normative encoding details before claiming full
standards conformance; the initial data-stage oracle check alone is insufficient.

### Capacity-planning checkpoint (QR-2A)

`qr--block-spec` takes version and exact level `/L`, `/M`, `/Q` or `/H`, returning
a fresh dictionary with `/Version`, `/Level`, `/Size`, `/RawCodewords`,
`/RemainderBits`, `/DataCodewords`, `/ECCodewordsPerBlock` and `/Blocks`.
Correction length is per block; raw codewords discard remainder bits.
`qr--byte-plan` takes a readable byte string and level, selects the smallest
fitting version and returns that layout plus fresh padded `/Data`. Neither
procedure is a public symbol API or draws anything. Executable strings/names
are treated as raw values using literal views, preserving caller attributes.

All 160 version/ECC specifications and 504 automatic plans at capacity
maximum-1/maximum/maximum+1 and count-width boundaries are frozen from the
independent encoder. Normal tests compare selected versions and SHA-256 of
actual padded data. Real Ghostscript checks table values, selected versions,
per-level overflow and executable/access/frame contracts. The original 98
data fixtures regenerate unchanged. The same-family review found no algorithm
blocker; it also discovered and prompted the preceding executable-string fix.
QR-2B correction/interleaving follows below.

### Error-correction checkpoint (QR-2B)

The library builds small GF(256) exp/log lookup tables using the QR polynomial
0x11D in ordinary PostScript. Internal `qr--rs-divisor` (degree 1–30) generates
the high-to-low coefficients excluding the leading monic term;
`qr--rs-remainder` returns fresh parity from readable data/divisor strings.
`qr--interleave` takes already padded data, explicit version and level; it
validates exact capacity, splits into short/long blocks, computes correction
and emits fresh interleaved data then correction columns. Remainder bits are
deliberately excluded from the byte stream. No matrix/rendering exists yet.

The correction reference corpus covers all 65,536 field products, 30 divisors,
91 remainders (including empty and maximum-length binary data), and full interleaved
stream hashes for every version/ECC combination. Synthetic data is deliberately
not periodic at 256 bytes, so truncated indexes are observable. Selected full
hex streams, including mixed blocks and version 40, also match Ghostscript.
The exact GitHub URL also matches independently generated complete codeword
streams at all four ECC levels, under PSCat and Ghostscript. This verifies the
data/correction pipeline, not scanning (there is still no module matrix).
All 15 QR tests and eight inline selftest blocks pass. No new dependency or
Rust encoder was added. Next: reserved patterns and fixed-mask matrices (QR-3).
The full locked offline CI-profile repository suite also passed, including the
38 selftest/manifest/strict-driver checks; all-target Clippy, formatting,
fixture regeneration, links, gallery and license checks passed. No rendered QR
acceptance is implied by the data-stage and correction-stage checks.

### Matrix checkpoint (QR-3, #970)

`qr--function-matrix` builds every function module and reservation for a
version; `qr--matrix` places a complete interleaved codeword stream in the
standard zigzag; `qr--apply-mask` applies one forced mask 0–7 to a fresh copy.
The representation is a byte-per-cell `/Cells` string with dark, function and
metadata flags ([API](QR_CODE_API.md#matrix-construction)). Format and version
cells are reserved light; their BCH bits, penalty scoring and automatic mask
selection are #971. All eight mask predicates landed here, since a fixed-mask
oracle comparison needs them.

Matrix fixtures come from the pinned oracle's real constructor at a fixed mask,
captured at its `_draw_codewords` and `_apply_mask` calls; no grid is produced
by our algorithm. All 40 versions are pinned for alignment centers and for the
function, metadata and fixed-pattern grids (full grids at versions 1, 2, 7, 10,
27 and 40); placement at versions 1, 2, 5, 7, 10, 14, 27 and 40 is pinned
unmasked and under all eight masks, with injected codewords so placement does
not depend on correction. Nine end-to-end symbols (including unequal-block
layouts and version 40) and the GitHub URL at L/M/Q/H go data -> correction ->
matrix -> mask. Ghostscript reproduces selected full grids and guards.

Coverage: empty and binary bytes, URLs, max/max+1 capacities, versions 1/9/10/
26/27/40, every ECC, unequal block groups, remainder bits, version BCH for >=7,
all masks and individual penalty rules. Cross-run PostScript under Ghostscript.
An independent decoder such as ZXing or ZBar must recover the exact GitHub URL
from actual PSCat PNG and rasterized SVG/PDF, at multiple scales. Missing a
decoder in the required verification job is a failure, not a green skip.

Main failure risks are bit order, GF polynomial 0x11D, unequal-block interleave,
reserved modules, top/bottom orientation, BCH placement, penalty edge cases and
quiet-zone sizing. Bound work by version <=40, symbol <=177 modules per side,
and byte capacity <=2953 for a single byte segment. No global scratch or
caller callbacks are needed; validate before opening private frames, and test
operand/dictionary/graphics stack preservation on success and dictionary
stack preservation on named failures (operand rollback after a failure is not
promised; see the API's validation section).

### Mask-selection checkpoint (QR-4, #971)

`qr--format-bits` and `qr--version-bits` compute the BCH(15,5) format word
(XOR `0x5412`) and BCH(18,6) version word; `qr--draw-metadata` writes both
format copies and both version blocks into the metadata cells of a masked
matrix. `qr--penalty` scores a grid with the reference encoder's four rules
(including its light-border convention for finder-like runs at the edge);
`qr--mask-penalties` scores all eight masks with their format bits drawn,
`qr--best-mask` takes the lowest (lowest mask number on a tie), and
`qr--finish`, `qr--codeword-symbol` and `qr--symbol` assemble complete symbols
([API](QR_CODE_API.md#metadata-penalties-and-mask-selection)).

All 32 format words and 34 version words, the oracle's eight trial scores and
its chosen mask, and the whole final grid are pinned for versions 1, 2, 7, 10,
27 and 40 at every level, for the GitHub URL at L/M/Q/H, and for a real
oracle tie away from mask 0. Thirty-two hand-built and seeded grids pin each
penalty rule's edges. Ghostscript reproduces the words, scores, grids and
guards. A `pscat --png` render of the GitHub URL with a four-module quiet zone
decodes to exactly `https://github.com/PSCatLabs/pscat` at all four levels
with the independent `rqrr` decoder, which also reports the expected version,
mask and level. This is the first scan proof; SVG/PDF and multi-scale decoding
are later work (#972, #973).

### Public API checkpoint (QR-5, #972)

`qrencode` and `qrdraw` are the public, catalog-tagged procedures
([contract](QR_CODE_API.md#public-api-qrencode-and-qrdraw)). Every argument is
checked on the operand stack before a frame, `gsave` or path change, so named
failures leave the dictionary stack and graphics state alone. `qrdraw` fills
one path of run rectangles whose corners are all `origin + k * module`, so
touching modules share exact edges: the seam pixel between two dark modules
reads 0 (black) in pscat's PNG, resvg's SVG raster and hayro's PDF raster,
including at a fractional 3.77 px/module pitch with a fractional origin. A
mutant that fills each run separately leaves grey seams there and fails.

Exports: for the GitHub URL at L/M/Q/H (4 px/module), the same URL at a
fractional pitch, and a version-10 payload with a forced mask, the PNG, SVG
and PDF exports are each rasterized (pscat; resvg 0.49.0; hayro 0.4.0, both
pure-Rust dev-dependencies) and checked module by module with row 0 at the
top, over the whole quiet zone, for seams, and by `rqrr` decoding the exact
text, version, mask and level. The catalog's own `load` and `example` lines
run from the embedded copy with the checkout hidden, and the skill's QR
block runs on a bare install. Ghostscript reproduces `/Modules` and the
state contract. A v40 symbol encodes and draws in about 0.5 s in PSCat.

## Business card target

Two 252×144 pt pages (3.5×2 in at 72 pt/in), front/back from one procedural
PostScript source. Investigate exporter page selection/naming before choosing
the gallery presentation: PDF can carry both sides; SVG/PNG need explicit side
outputs plus a gallery proof. Each exported side must preserve physical units;
PNG at 300 dpi is 1050×600 pixels and needs an explicit print-size recipe.

Design direction: restrained ink/navy, warm white, a typographic PSCat front
and a clear invitation to explore programmable graphics on the reverse.
Aim for a >=0.8 inch high-contrast QR including its quiet zone, exact size
checked after version selection; keep text >=9 pt and critical art >=9 pt
inside trim. Start with white edges/no bleed requirement; add a separate
printer-specific bleed/imposition recipe only if needed. Proof actual exported
pages at card size before gallery registration. No raster art or promotional
claims beyond what PSCat currently does.

## Session infrastructure

The Mac rebooted after the first clone hit disk exhaustion. The root checkout
was recovered without source changes; main was not committed or modified.
The external drive is `/Volumes/YOTUO` (about 641 GiB free at inspection).
`target` is an ignored local symlink to `/Volumes/YOTUO/pscat-qr-code/target`;
bootstrap with `cargo build --target-dir /Volumes/YOTUO/pscat-qr-code/target`.
Git/reference copies live under `/Volumes/YOTUO/pscat-qr-code/`.

Ordinary external/home cache writes are sandbox-restricted. No global caches
were deleted or relocated: `~/.cache` measured about 23 GiB, Cargo registry
about 1 GiB, Library/Caches about 11 GiB. Use the authorized Cargo invocation
for external target output; broader cache relocation needs writable workspace
access. Do not change tracked paths into machine-specific symlinks.

On 2026-10-28 (history), GitHub issue publication failed: connector create returned HTTP 403
(`Resource not accessible by integration`). The requested CLI fallback was
attempted: `gh api`/`gh issue list` cannot connect and `git push` cannot resolve
GitHub inside the sandbox; `git fetch` has a working preapproved rule. Outside-
sandbox CLI access was rejected by the session approval policy. Consequently,
the earlier `gh auth status` invalid-token message does not establish that the
credentials need replacing. This was later resolved: the checkpoint is draft PR
#957 and the queue is filed as epic #966 with tickets #967–#974.
This is an infrastructure limitation, not an exception to verification gates.
