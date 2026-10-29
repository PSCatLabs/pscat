# Native PostScript QR data and correction pipeline — draft checkpoint

## Summary

PSCat needs a reusable native PostScript QR library for its core distribution
and a printable promotional card. This draft implements the data/correction
foundation so it can receive independent Opus review before matrix work begins.

- Adds byte-mode packing, smallest-version selection across versions 1–40,
  exact L/M/Q/H, GF(256), Reed–Solomon parity and unequal-block interleaving.
- Encoding lives entirely in `lib/qr.ps`; there is no QR Rust primitive,
  engine change or new dependency. Existing resource embedding supplies it.
- Adds guards/selftests, 15 integration tests and pinned independent reference
  fixtures. Executable byte strings remain raw data and preserve caller state.
- Includes implementation contracts, reproducible verification, a phased issue
  queue and a dedicated Opus review handoff.

**Draft / do not merge:** matrix construction, format/version metadata, masks,
vector rendering, the public API, actual image decoding and the two-sided card
remain pending. Matching codewords is not a scannable QR or a full standards-
conformance claim. Main and other ongoing feature branches remain unchanged.

## Opus review

Start with the
[review handoff](https://github.com/PSCatLabs/pscat/blob/feature/qr-code-library/docs/dev/design/QR_CODE_REVIEW.md),
[implemented API](https://github.com/PSCatLabs/pscat/blob/feature/qr-code-library/docs/dev/design/QR_CODE_API.md),
and [verification guide](https://github.com/PSCatLabs/pscat/blob/feature/qr-code-library/docs/dev/design/QR_CODE_TESTING.md).
Record the reviewed PR head. Review byte/count boundaries, all-version layout
facts, GF/RS/interleave correctness, executable attributes/private-frame
unwinding, input/output aliasing, oracle independence and embedded loading.
Do not treat deferred stages as bugs in the current checkpoint. No Opus review
has been completed yet.

Base: `07f054c19d7045281a667e185b9bfc74f3cde29e`.
Last implementation commit: `deba699b4dd221176056f37d05761a55c8126c45`.
Documentation-only preparation commits follow it.

## Test evidence

- [x] Locked offline CI-profile build and full repository suite.
- [x] All 15 QR integration tests; actual Ghostscript checks executed.
- [x] Eight inline selftest blocks; all 38 manifest/strict-driver checks in Rust.
- [x] All-target Clippy with warnings denied; formatting check.
- [x] Markdown links, gallery manifest, licenses and fixture regeneration.
- [ ] Exact shell selftest wrapper: its Cargo build-lock access was sandbox-
  denied; the underlying checks passed through `tests/selftest.rs`.
- [ ] Remote CI: results must be observed after this PR is created.
- [ ] Independent Opus review, then any required fixes/retests.

Reference coverage: 98 byte vectors, 160 block layouts, 504 automatic boundary
plans, all 65,536 field products, 30 divisors, 91 remainders, all 160 interleaved
streams, nine full hex streams and the exact GitHub URL at all four ECC levels.
Normal tests are offline; optional regeneration verifies a separate pinned
Nayuki checkout. Missing Ghostscript can skip portability tests, so the
verification guide distinguishes that from the observed local GS passes.

## Tracking and next steps

The [issue/dependency queue](https://github.com/PSCatLabs/pscat/blob/feature/qr-code-library/docs/dev/design/QR_CODE_ISSUES.md)
contains the epic and eight implementation tickets with acceptance criteria.
No GitHub issue numbers have been assigned because publication has been
blocked; this PR must not imply that any nonexistent issue was closed.
Next is QR-3 reserved patterns/fixed-mask placement, then metadata/masking and
the first independent decode before drawing/gallery/card work.

The functional PostScript experiment is a separate branch, with no dependency,
copy or merge introduced here. The card can use main's existing typography.

## Checklist

- [x] Documentation matches this implemented checkpoint and states limitations.
- [x] Code/tests/docs are contributed under MIT OR Apache-2.0, as CONTRIBUTING.md specifies.
- [x] No CODEOWNERS-protected path changed at this checkpoint.
- [x] Draft only; no main merge is authorized.
