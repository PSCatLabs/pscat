# QR checkpoint verification

Scope: [implemented data/correction API](QR_CODE_API.md) through implementation
commit `deba699`, plus the subsequent review documentation. The
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
python3 scripts/pr_body_lint.py docs/dev/design/QR_CODE_PR.md --git-range origin/main..HEAD
```

Normal Rust tests use checked-in JSON and need no network, oracle checkout,
pip package or new dependency. `--offline` can be added to Cargo commands if
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
| Final QR integration suite | 15 tests passed; no failed or ignored QR tests. |
| Real Ghostscript portability/reference checks | Executed and passed on the Mac, not skipped. |
| Inline `--selftest lib/qr.ps` | Eight blocks passed. |
| Manifest/strict-driver suite | All 38 `tests/selftest.rs` checks passed in the full run. |
| All-target Clippy, warnings denied | Passed. |
| Formatting, Markdown links, gallery manifest, licenses | Passed. |
| Three independent fixture regenerations | Matched the committed files. |
| Shell `scripts/selftest.sh` wrapper | Its Cargo build-lock access was sandbox-denied; do not report this wrapper as passed. Underlying manifested-library and strict-driver checks passed in Rust. |
| Remote PR CI | Not observed: publication is currently blocked by CLI network access. |

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

No QR matrix or render exists. There is no independent decoding result, quiet-
zone proof, SVG/PDF/PNG QR export, physical-card sizing check or card artwork.
No claim of complete ISO/IEC 18004 conformance or scanner reliability is made.
The current stage verifies data encoding and correction only. Future required
decode tests must fail if the decoder is unavailable; they must not inherit
the optional-Ghostscript skip convention.

Cross-platform, headless-package, MSRV, crate/publishing and full remote CI
results have not been observed for this branch. Existing CI should supply
those results after publication. Normative standards checks and independent
cross-family review remain outstanding before any merge.
