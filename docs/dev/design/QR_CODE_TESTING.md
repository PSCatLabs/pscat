# QR checkpoint verification

Scope: [implemented data/correction/matrix API](QR_CODE_API.md): data and
correction through commit `deba699`, plus matrix construction (#970) and
metadata, penalties, mask selection and the first decode proof (#971), plus
the public `qrencode`/`qrdraw` API and its PNG/SVG/PDF export checks (#972), plus
the all-version boundary corpus, second decoder and physical checks (#973). The
[Opus handoff](QR_CODE_REVIEW.md) gives the review scope and expected report.

## Reproduce normal checks

From the repository root, with stable Rust, Python 3 and optional Ghostscript:

```sh
cargo build --locked --profile ci
cargo test --locked --profile ci --test qr
cargo test --locked --profile ci --test qr_corpus
cargo test --locked --profile ci --test qr_card
# The exhaustive render/decode sweep (#973), about 20 s on 8 threads:
cargo test --locked --profile ci --test qr_corpus -- --ignored --nocapture
cargo test --locked --profile ci
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
PSCAT_PROFILE=ci ./scripts/selftest.sh
python3 scripts/check_md_links.py
python3 scripts/gallery.py check
python3 scripts/check_licences.py all
python3 scripts/codeowners_check.py --base origin/main
# pr_body_lint.py accepts empty stdin, so fetch to a file and check it first.
body=$(mktemp "${TMPDIR:-/tmp}/qr-pr-body.XXXXXX") && gh pr view 957 --json body --jq .body > "$body" && test -s "$body" && python3 scripts/pr_body_lint.py "$body" --git-range origin/main..HEAD
```

Normal Rust tests use checked-in JSON and need no network, oracle checkout or
pip package. The test-only dependencies are Rust dev-dependencies that Cargo
builds into the test binary: the `rqrr` QR decoder (#971), the `resvg`
SVG and `hayro` PDF rasterizers (#972) and the `rxing` QR decoder (#973), all
pinned exactly. `--offline` can be added to Cargo commands if
the existing dependencies are cached. Existing standard-35 fonts are embedded;
no optional font pack is required. Review `git status` after checks.

On this Mac, ignored local `target` points to
`/Volumes/YOTUO/pscat-qr-code/target`. The verified Cargo invocations explicitly
used `--target-dir /Volumes/YOTUO/pscat-qr-code/target`. Other machines should
use their own target directory; no machine-specific build configuration is
committed. The oracle checkout is also on that external drive.

## Recorded local evidence

| Check | Observed outcome |
|---|---|
| Locked CI-profile build | Passed. |
| Full locked offline CI-profile repository suite | Passed, exit 0. |
| Final QR integration suite | 15 tests passed; no failed or ignored QR tests. #970 adds seven (22 total), #971 seven more (29), all passing with Ghostscript present. |
| Real Ghostscript portability/reference checks | Executed and passed on the Mac, not skipped. |
| Inline `--selftest lib/qr.ps` | Eight blocks passed; 13 after #970, 21 after #971. |
| Manifest/strict-driver suite | All 38 `tests/selftest.rs` checks passed in the full run. |
| All-target Clippy, warnings denied | Passed. |
| Formatting, Markdown links, gallery manifest, licenses | Passed. |
| Three independent fixture regenerations | Matched the committed files. |
| Shell `scripts/selftest.sh` wrapper | Initially sandbox-denied (Cargo build lock); later passed at `5d0d705` in the [checkpoint review](https://github.com/PSCatLabs/pscat/pull/957#pullrequestreview-5743891246). |
| Remote PR CI | Observed on #957: `cargo test` 2590 passed, 0 failed ([run 41146962880](https://github.com/PSCatLabs/pscat/actions/runs/41146962880)). Whether CI's macOS runner had `gs` is not shown, since passing tests' skip messages are captured. |

The Rust Ghostscript tests follow existing repository convention: they return
early if `gs` cannot be launched. Therefore a green suite on a machine without
Ghostscript does not prove portability. Check `gs --version` and run the three
`ghostscript_*` QR tests with `--nocapture` when reproducing that evidence.

## Independent fixtures and provenance

Oracle: [Nayuki QR-Code-generator](https://github.com/nayuki/QR-Code-generator),
revision `3c6d0b3cefb4e049dc337e82237c9644399716a8`.
The generator verifies the checkout commit and exact Python source bytes before
importing it; each fixture file records the revision and source SHA-256.
No oracle implementation is vendored or used at runtime. Only factual outputs
are frozen. The oracle's own source is MIT licensed in its separate checkout.

```sh
git clone https://github.com/nayuki/QR-Code-generator.git /path/to/qr-reference
git -C /path/to/qr-reference switch --detach 3c6d0b3cefb4e049dc337e82237c9644399716a8
python3 scripts/qr_reference_vectors.py /path/to/qr-reference --check
```

Omit `--check` only when intentionally regenerating fixtures, then inspect the
diff. Running the generator never fetches or installs packages. Tests do not
regenerate their own expected outputs.

| Fixture | Coverage / independent boundary |
|---|---|
| [Byte vectors](../../../tests/data/qr_byte_vectors.json) | 98 empty, maximum binary and GitHub URL cases at selected versions, every ECC; captures real padded data before matrix construction. |
| [Planning vectors](../../../tests/data/qr_planning_vectors.json) | All 160 layouts plus 504 automatic selections at maximum-1/maximum/maximum+1, short and count-width cases; expected versions come from actual automatic encoding, with padded-data SHA-256. |
| [Correction vectors](../../../tests/data/qr_correction_vectors.json) | All 65,536 field products by hash; 30 divisors, 91 remainders, all 160 interleaved-layout hashes, nine full hex streams, and four complete GitHub URL streams. |
| [Matrix vectors](../../../tests/data/qr_matrix_vectors.json) | Alignment centers and function/metadata/fixed-pattern grids for all 40 versions (full grids at 1, 2, 7, 10, 27, 40); placement at eight versions unmasked and under all eight masks; nine end-to-end symbols including unequal blocks; the GitHub URL at L/M/Q/H. Captured from the oracle's real constructor at a fixed mask. |
| [Corpus vectors](../../../tests/data/qr_corpus_vectors.json) (#973) | All 160 version/level cells: a binary payload of exactly the version's byte capacity at a forced mask (the oracle's complete codeword stream and finished grid), and one byte more with automatic masking (the next version, or overflow at 40; codewords, the eight trial scores, chosen mask, grid). Five URL/UTF-8 texts at L/M/Q/H. Hashes only. |
| [Mask vectors](../../../tests/data/qr_mask_vectors.json) | All 32 format and 34 version words; automatic-mask symbols at versions 1, 2, 7, 10, 27 and 40 for every level, the GitHub URL at L/M/Q/H and a real tie (masks 1 and 5) with the oracle's eight trial scores, chosen mask and full final grid; 32 hand-built and seeded penalty grids. |

Byte/planning generation explicitly constructs one BYTE segment, including for
empty input, fixes ECC with boosting disabled and captures the constructor
input. Planning allows the oracle to choose its own version; it does not repeat
our capacity-search algorithm. Correction generation calls the oracle's actual
divisor, remainder and interleaving methods, not a copied splitting algorithm.
The selected-mask parameter is fixed at zero only to make capture deterministic;
these fixtures do not test matrix masking.

Interleave inputs use `(149*i + floor(i/251) + 17*version + 23*levelOrdinal)
mod 256`. They are deliberately not periodic every 256 bytes, exposing truncated
indexes. Long data and unequal block groups are included. Both all-short and
mixed groups are covered.

Matrix generation runs the oracle's real constructor with a fixed mask and
records its own intermediate grids: the function grid and fixed values when it
calls `_draw_codewords`, and the module grid before and after `_apply_mask`.
Metadata cells are those written by its format/version routines, except the
one cell dark for every level and mask (the dark module), found empirically
across 32 runs per version. Format/version cells are cleared to light in the
compared grids until #971 computes their BCH bits. Placement fixtures inject
`(149*i + floor(i/251) + 31*version) mod 256` codewords in place of the
oracle's correction output, so a placement failure cannot be a correction
failure. Full grids are stored packed (one bit per cell, row-major) where a
mismatch should name the cell; the rest are SHA-256 of one byte per cell.

Mask generation (#971) reads format/version words from the order the oracle's
own `_draw_format_bits`/`_draw_version` write modules on a bare symbol (bit
`i` is the `i`-th write; the second copy must repeat it). Symbols run the real
constructor with automatic masking while a wrapper records each
`_get_penalty_score` result, so the scores are the oracle's own trials. Final
grids are the finished module grid, metadata included, with no clearing.
Penalty grids are scored by calling the oracle's `_get_penalty_score` on a
bare object: finder-like runs at the left/right/top edges, with four, three,
one and no light modules beside them, both-sides double counts, a doubled
scale, a wrong centre, runs of four to seven, 2×2 blocks, dark counts on both
sides of every N4 band edge, and seeded noise.

## Decode proof

`rendered_github_symbols_decode_independently_at_every_level` renders the
GitHub URL with `pscat --png`: an opaque white page, a four-module quiet zone,
four device pixels per module at 72 dpi. It loads the PNG, composites it onto
white, and decodes it with [`rqrr`](https://crates.io/crates/rqrr) 0.11.0
(`default-features = false`, MIT OR Apache-2.0 and ISC). It asserts the exact text, version, mask and format
level at L/M/Q/H. The decoder is compiled into the test binary, so this test
has no skip path: if it cannot decode, the suite fails. `rqrr` is a separate
implementation (based on quirc); it shares no code with `lib/qr.ps` or the
fixture oracle.

## Export proof (#972)

`png_svg_and_pdf_exports_keep_the_matrix_and_quiet_zone_and_decode` renders
each case with `pscat --png`, `--svg` and `--pdf` (one unique
`common::scratch_dir` per child, the standard child deadline) and rasterizes
the SVG with [`resvg`](https://crates.io/crates/resvg) 0.49.0 and the PDF with
[`hayro`](https://crates.io/crates/hayro) 0.4.0 (resvg MIT OR Apache-2.0,
hayro 0.4 Apache-2.0; pure Rust, no default features). hayro is held at 0.4:
0.5 and later switch flate2 to zlib-rs, which through feature unification
changes the compression of the pscat binary the tests run (a `tests/mcp.rs`
inline-size limit failed), and 0.7 and later need rustc 1.92.
Neither shares code with pscat's rasterizer. For each raster it checks:

- every module centre against `/Modules`, mapping row 0 to the top of the
  raster (a flipped symbol still decodes, so decoding alone would not catch
  it), and every quiet-zone module light;
- the pixel straddling each edge between two dark modules is black (≤ 8):
  the whole symbol is one path filled once, so these read exactly 0, while
  filling runs separately leaves grey seams at a fractional pitch;
- `rqrr` finds exactly one symbol and decodes the exact text, version, mask
  and level.

Cases: the GitHub URL at L/M/Q/H at 4 px/module; at M with a 2.9 pt module,
3.3 pt origin and 1.3x scale (3.77 px/module, nothing on the pixel grid); a
139-byte URL forced to version 10 (version-information blocks), level Q,
mask 5, at 5.7 px/module. Failures name the fixture, version, level, mask,
pitch, export and failing stage, e.g. `github-M-fractional v3 M mask 0 (2.9
pt/module, x1.3): PNG: seam below (0, 2), grey 61`. The decoder and both
rasterizers are compiled in, so this test cannot skip. `rqrr` (quirc-based)
fails to find a version-10 symbol at about 4 px/module in any of the three
rasters, so that case uses 5.7 px/module; this is a decoder limit, not a
rendering fault, and #973's corpus should add a second decoder.

`svg_export_is_one_light_square_and_one_path_of_run_rectangles` checks the
vector geometry: one light path, one dark path whose subpaths equal the
symbol's horizontal dark runs. `light_null_leaves_the_surface_below_unpainted`
checks `/Light null`. `tagged_capabilities_run_from_the_embedded_distribution`
reads `qrencode`/`qrdraw` from `pscat --capabilities`, runs their `load` and
`example` lines in an empty directory with `PSCAT_NO_CHECKOUT=1`, asserts the
`lib/qr.ps <- embedded` trace and decodes the example's PNG.

## Boundary corpus (#973)

[`tests/qr_corpus.rs`](../../../tests/qr_corpus.rs) covers every version 1–40 at
every level in the default suite, with no rendering. For each cell the corpus
payload is a binary byte string of exactly the version's byte-mode capacity
(byte `i` = `(149i + ⌊i/251⌋ + 7v + 41·ordinal) mod 256`: every byte value,
NUL and 0x80–0xFF included, not periodic at 256). Its SHA-256 is pinned too,
so the Rust copy of the formula cannot drift from the generator's. Per cell:

- the planner picks that version, and the interleaved codeword stream
  matches the oracle's (`codewords_sha256`, captured at its
  `_draw_codewords`);
- `qrencode` at the forced mask `(v + ordinal) mod 8` gives the oracle's whole
  finished grid, metadata included: a fixed-mask full-matrix comparison for
  all 160 layouts, beyond #970/#971's six versions;
- one byte more moves to the oracle's next version with its codewords, chosen
  mask and grid (a mismatch prints both sides' eight penalties), or raises
  `qr-data-too-long-for-level` at version 40;
- that byte-longer payload at a fixed `/Version v` raises
  `qr-data-too-long-for-version`.

All 40 versions of one level run in one interpreter, which checks the
dictionary depth after each, and re-encodes the first symbol at the end
(repeated calls). One `#[test]` per level runs them in parallel. URLs (12,
34, 304 and 1,222 bytes, the last reaching version 40 at H) and a raw UTF-8
URL with Latin, CJK and Thai text match the oracle with automatic version and
mask. A table of invalid options (lower-case, long, numeric and null levels,
negative/real/string/boolean versions, real and name masks, non-dict
options, non-string payloads) and each level's v40 overflow (at +1 and +1000
bytes) raise named errors without leaving a frame. The 255-byte count-width
edge is checked as well. The fixtures regenerate byte-identically, as do the
five earlier files.

## Decode corpus and per-decoder results (#973)

A second decoder joins rqrr: [`rxing`](https://crates.io/crates/rxing) 0.9.3,
a pure-Rust port of ZXing (Apache-2.0, MSRV 1.85), with only its QR reader
(`qrcode`, `decoders`, `encoding_rs`). rqrr ports quirc, so the two share
no detector, sampler or Reed–Solomon code. They read different fields, and
each check asserts only what that decoder reports. rqrr reports the payload
(read through its byte writer, not its UTF-8 API), version, mask and level.
rxing reports the payload (its byte-segment metadata, not its
charset-guessed text), the level and the corrected data codewords, which
must equal the encoder's padded data. Both run without a pure-barcode hint,
so their finder detectors locate the symbol. The shared rasterizers and
decoders live in [`tests/qr_support`](../../../tests/qr_support/mod.rs).

Before decoding, each raster is checked pixel by pixel. A pixel lying wholly
inside one kind of area (page, light square, dark modules, possibly spanning
several modules of that kind) must have that area's exact grey (±8).
Only pixels that straddle an edge between areas may be grey. This
pins the quiet-zone width, every module edge and seam-free dark runs at any
pitch, in pscat's PNG and in resvg's and hayro's rasters. pscat's PNG rounds
the page to whole pixels (in `f32`) with y = 0 on the bottom pixel edge;
resvg and hayro map the page exactly. The check follows each convention.

Default suite (`rendered_subset_decodes_with_both_independent_decoders`, about
4 s): versions 1, 2, 6, 7, 9, 10, 14, 21, 27 and 40 across the four levels,
which covers the version-information (6/7) and count-width (9/10) boundaries and
unequal block groups. Each is rendered at 2 and 3 px/module on the pixel grid
and at 3.77 px/module from a fractional origin, through all three exports.
Both decoders must read every pixel-aligned raster. At the fractional pitch,
the rasters must still pass the pixel check, but decoder outcomes are only
printed (24 misses on macOS arm64). Which symbols a decoder misses there
depends on edge-pixel greys, and CI also runs this test on Linux and
Windows.

Exhaustive sweep (`--ignored`): all 160 capacity symbols at seven pitches,
2,080 rasters and 4,160 decodes, in 22.7 s on 8 threads (Apple M-series, `ci`
profile). Every raster passed the pixel check. It fails if a raster is not
pixel-exact or if both decoders miss a pixel-aligned raster. Measured results
(symbols read, of 160):

| px/module | grid | export | rqrr | rxing |
|---|---|---|---|---|
| 2.00 | aligned | PNG, SVG, PDF | 160 each | 160 each |
| 3.00 | aligned | PNG, SVG, PDF | 160 each | 156 each (v20-M, v24-H, v31-Q, v37-L) |
| 4.00 | aligned | PNG | 160 | 153 |
| 2.60 | fractional | PNG | 0 | 102 |
| 3.77 | fractional | PNG / SVG / PDF | 76 / 86 / 82 | 102 / 99 / 128 |
| 4.50 | fractional | PNG | 68 | 132 |
| 5.70 | fractional | PNG | 152 | 153 |

Every capacity symbol is read by both decoders at 2 px/module on the pixel
grid, and by rqrr at every whole-pixel pitch. Misses happen only in the
decoders: the rasters are pixel-exact, and three unrelated rasterizers agree.
Two probes, run once by hand and not kept in the suite, back this up.
Thresholding the fractional rasters to pure black and white changes which
symbols each decoder misses, without fixing them. Sweeping the 156
automatic-mask symbols one byte past capacity instead of the forced-mask
ones gives the same miss rates: rqrr 0/156 at 2.6 px and 73–77/156 at 3.77
px; rxing 94–121/156 at 3.77 px, and 153/156 even at 2 px on the grid. The
misses do not cluster by mask number, so the forced masks are not the cause. Both decoders sample a
grid estimated from the finder patterns, and when module edges fall inside
pixels, large dense symbols (high versions, full capacity) are where that
estimate fails. rqrr is the weaker at small fractional pitches; rxing has a
few whole-pixel misses. So this is the
practical guidance: a raster (PNG) meant for screens should use a whole
number of device pixels per module on the pixel grid; print at printer
resolution has many pixels per module. This generalizes #972's note that
rqrr missed the version-10 case at about 4 px/module fractional.

## Physical size, print recipe, quiet zone and module edges (#973)

- `card_exports_keep_physical_size_and_the_300_dpi_print_recipe`: a 3.5 × 2 in
  card (252 × 144 pt) with a mid-grey backdrop and a 0.8 in (`/Side 57.6`)
  GitHub symbol at (180, 72). The PDF MediaBox is `[0 0 252 144]`, and the SVG
  is `width="252" height="144" viewBox="0 0 252 144"`. The print recipe
  `pscat --page 252x144 --dpi 300 --png card.png card.ps` gives 1050 × 600
  pixels. In all three rasters at 300 dpi, the light square is exactly
  pixels 750–989 × 60–299 (0.8 in = 240 px), pixel-checked and decoded by both
  decoders.
- `quiet_zone_and_module_edges_land_exactly_at_every_scale`: the GitHub URL
  at Q with quiet zones of 4, 5 and 6 on a grey backdrop, at 1, 2, 3, 3.54,
  3.77, 4.17 and 6.8 px/module (whole and fractional origins). The 0.85 pt
  (0.3 mm) print rule of thumb at 300 dpi is one of them. Every raster in all
  three exports is pixel-checked; from 2 px/module up, both decoders read it.
  A quiet zone below 4 is refused (`qr-quiet-zone-must-be-at-least-4`, the
  #972 guards).

## The gallery business card (#974)

`tests/qr_card.rs` checks `gallery/pscat_card.ps`, the two-sided card, through
what a user exports, not a test program:

- `every_export_has_two_card_sized_sides_and_the_back_scans`. The card is
  exported through `--pdf` (two `/MediaBox [0 0 252 144]` pages,
  `--lint-strict` clean, no image XObject), `--svg` (`card-001.svg` and
  `card-002.svg`, 252 × 144, no `<image>`) and `--png --dpi 300`
  (`card-001.png` and `card-002.png`, 1050 × 600). Each side of each export is
  rasterized at 300 dpi: pscat's PNG, the SVG through resvg, each PDF page
  through hayro (`qr_support::raster_pdf_page`). Then:
  - every pixel within 9 pt of the trim is paper white;
  - on the back, every pixel of the 41 × 41-module drawing, quiet zone
    included, is exactly its module's light or navy, since 1.68 pt modules
    at (171.12, 37.44) pt are 7 whole pixels;
  - on the back, both decoders read the exact URL at level H (rqrr: version
    4);
  - on the front, neither decoder finds a symbol.
- `the_gallery_still_is_a_scannable_proof_of_both_sides`: the committed proof
  still (1250 × 1500 px at 300 dpi) gets the same pixel check at the back's
  proof offset (24, 24) pt. Both decoders read it, and rqrr finds exactly one
  symbol.
- `the_card_uses_base_35_fonts_at_9_pt_or_more_and_no_images`. The card is
  run with `findfont`, every show operator and the image operators wrapped
  to report each use. Every face must be one of the LaserWriter 35 (compiled
  in, so no font pack). Every show's em, through the font matrix and the CTM,
  must be at least 9 pt. Nothing may paint a raster.

## Standards references

- ISO/IEC 18004:2015, *QR Code bar code symbology specification*: the
  normative source for Model 2 versions and capacities, error correction
  blocks, format and version information, alignment pattern positions,
  masking and penalty rules and the four-module quiet zone. The library and these
  tests were not checked clause by clause against the purchased text.
  The tables are verified against the independent oracle, and the oracle
  follows the standard.
- DENSO WAVE, [QR code size and quiet zone](https://www.qrcode.com/en/howto/code.html)
  (module sizing, four-module margin).
- Oracle: [Nayuki QR-Code-generator](https://github.com/nayuki/QR-Code-generator)
  at the pinned revision above. Decoders: rqrr 0.11.0 (quirc lineage) and
  rxing 0.9.3 (ZXing lineage).

## Limitations

- Byte mode only, one segment, no ECI. Numeric, alphanumeric and kanji
  modes, mode mixing, Structured Append and Micro QR are out of scope.
- Conformance is shown by agreement with one independent encoder plus two
  decoders, not by a certification test suite. No scanner (phone camera)
  reliability is claimed. The decoders' misses at fractional pitches above
  are a reminder that cameras differ.
- The SVG has no physical unit: `width`/`height` are user units (points)
  without `pt`, so an SVG consumer at 96 px/in shows a 252-unit card at 2.625
  in, not 3.5 in. Print from the PDF, which is exact, or scale the SVG by 4/3.
  The PNG carries no `pHYs` resolution chunk: its print size is its pixel
  count divided by the dpi it was rendered at, which the print dialog must
  be told.
- The suite's QR tests take about 12 s of wall time (`ci` profile, about 45 s of
  CPU across four parallel level tests). An unoptimized `cargo test` is many
  times slower.

## Behavioral and portability assertions

The [Rust tests](../../../tests/qr.rs) also check input immutability, fresh
outputs, repeated calls, caller-name collisions, executable strings/names,
named type/range/fit failures, direct post-error dictionary depth, and embedded
loading from a unique scratch directory outside the checkout. All child
processes use `tests/common` deadlines; scratch paths use `common::scratch_dir`.

Selftest helpers restore stacks after caught failures. Consequently, direct
dictionary-depth assertions are retained in Rust/GS tests so harness cleanup
cannot conceal a leaked `begin`. Ghostscript separately exercises unreadable
strings, because PSCat access operators currently do not enforce real read
protection. The maximum accepted RS input of 2956 bytes and rejection at 2957
are both pinned.

## What remains unverified

Every version and level matches the oracle, and the rendered corpus decodes
with two independent decoders (#973). The card's exports decode too (#974). No claim of
complete ISO/IEC 18004
conformance or scanner reliability is made. Later decode tests must, like
#971's, fail if the decoder is unavailable; they must not inherit the
optional-Ghostscript skip convention.

Remote CI on #957 passed, but that alone does not prove Ghostscript
portability (see above). Normative standards checks remain outstanding, and
each later stage (#970–#974) needs its own independent review before any merge
to `main`.
