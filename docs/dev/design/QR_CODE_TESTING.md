# QR checkpoint verification

Scope: [implemented data/correction/matrix API](QR_CODE_API.md): data and
correction through commit `deba699`, plus matrix construction (#970) and
metadata, penalties, mask selection and the first decode proof (#971), plus
the public `qrencode`/`qrdraw` API and its PNG/SVG/PDF export checks (#972). The
[Opus handoff](QR_CODE_REVIEW.md) gives the review scope and expected report.

## Reproduce normal checks

From the repository root, with stable Rust, Python 3 and optional Ghostscript:

```sh
cargo build --locked --profile ci
cargo test --locked --profile ci --test qr
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
builds into the test binary: the `rqrr` QR decoder (#971), and the `resvg`
SVG and `hayro` PDF rasterizers (#972), all pinned exactly. `--offline` can be added to Cargo commands if
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
[`hayro`](https://crates.io/crates/hayro) 0.5.0 (both MIT OR Apache-2.0, pure
Rust, no default features; hayro is held below 0.7, which needs rustc 1.92).
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

The public API draws symbols whose PNG, SVG and PDF exports decode with one
independent decoder at two pitches (#972). There is no all-version decode
sweep, multi-decoder corpus, physical-card sizing check or card artwork
(#973, #974). No claim of complete ISO/IEC 18004
conformance or scanner reliability is made. Later decode tests must, like
#971's, fail if the decoder is unavailable; they must not inherit the
optional-Ghostscript skip convention.

Remote CI on #957 passed, but that alone does not prove Ghostscript
portability (see above). Normative standards checks remain outstanding, and
each later stage (#970–#974) needs its own independent review before any merge
to `main`.
