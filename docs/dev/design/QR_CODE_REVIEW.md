# Opus review handoff: native QR data/correction checkpoint

Review target: repository `PSCatLabs/pscat`, branch `feature/qr-code-library`.
This is a draft, incomplete feature checkpoint. Do not merge it. No Opus review
has yet been run or recorded; this document prepares that independent review.

Base: `07f054c19d7045281a667e185b9bfc74f3cde29e` (main at branch creation).
Last implementation commit: `deba699b4dd221176056f37d05761a55c8126c45`.
Subsequent preparation changes are documentation and publication tooling. Record the actual
checked-out/reviewed head in the review, and inspect the complete PR diff.

## Review scope

Review the implementation of QR-1, QR-2A and QR-2B: pure PostScript byte-mode
packing, version/capacity planning across versions 1–40 at exact L/M/Q/H,
GF(256), Reed–Solomon parity and equal/unequal block interleaving. Include tests,
fixture provenance, distribution through the existing loader and the accuracy
of the documentation. No Rust engine/operator/dependency changes are included.

The project goal remains a complete Model 2 library plus a procedural two-sided
PSCat business card. Matrix patterns/placement, BCH format/version metadata,
mask predicates/scoring, public API, vector drawing, scan verification,
SVG/PDF/PNG QR exports and the card are explicitly pending. Their absence is
not a regression in this checkpoint; assess whether the current implementation
and planned boundaries support adding them correctly.

## Read in this order

1. [Design and remaining sequence](QR_CODE.md), then
   [implemented API contracts](QR_CODE_API.md).
2. [Library](../../../lib/qr.ps), the entire file. It is small enough to review
   directly; no generated QR algorithm is hidden in Rust.
3. [Behavioral tests](../../../tests/qr.rs) and
   [oracle generator](../../../scripts/qr_reference_vectors.py).
4. [Verification record and reproduction](QR_CODE_TESTING.md), then inspect
   representative [byte](../../../tests/data/qr_byte_vectors.json),
   [planning](../../../tests/data/qr_planning_vectors.json) and
   [correction](../../../tests/data/qr_correction_vectors.json) vectors.
5. [Issue/dependency queue](QR_CODE_ISSUES.md),
   [development notes](../../../notes/qr-code-library.md), and the full diff.

The issue queue has local IDs because publication has been blocked. Do not
invent issue or PR numbers. The separate functional PostScript branch was
inspected but is neither merged nor copied; the QR code has no dependency on it.

## Questions for the review

- Is byte packing correct at both count widths, empty input, binary bytes,
  exact fits and pad-byte parity? Is the explicit-capacity seam kept distinct
  from standards capacity selection?
- Are all block counts/correction lengths and the raw-module calculation
  correct, including discarded remainder bits and high versions? Does planning
  select the smallest fitting version without changing the requested ECC?
- Are finite-field multiplication, generator coefficient order and polynomial
  division correct at zero and degree boundaries? Are short/long block splits
  and the skipped short-block extra data column correct?
- Can an executable string/name run accidentally, or can ordinary validation
  leak a private dictionary? Are read access, caller-state preservation, fresh
  output ownership and failure contracts accurately documented?
- Are the frozen expectations independent of our algorithms? Do hashes and
  nonperiodic input expose likely index/interleave mistakes? Are tests silently
  skipped, tautological, or able to pass despite a wrong implementation?
- Does `(lib/qr.ps) run` work from the existing embedded distribution without
  new primitives or loader tables? Are all new definitions tagged internal?
- Are any premature public API or conformance claims made? Identify structural
  changes needed now before matrix/rendering work makes them harder to undo.

## Commands and observed evidence

Use the [verification guide](QR_CODE_TESTING.md). At minimum, build and run the
15-test QR integration suite plus `--selftest lib/qr.ps` under PSCat and the
actual Ghostscript cases. Full repository tests, Clippy, formatting, fixture
regeneration, links, gallery and licenses passed locally. Remote CI has not
been observed, and the shell selftest wrapper had a sandbox Cargo-lock denial;
its underlying 38 manifest/strict-driver checks passed through Rust. Do not
convert that limitation into a claim that the wrapper passed.

For a source review without running tests, explicitly say tests were not run.
If Ghostscript is absent, say so; green optional tests do not prove portability.
No scan result is expected until the future symbol implementation exists.

## Suggested review request

> Review PSCatLabs/pscat branch feature/qr-code-library independently. Start at
> docs/dev/design/QR_CODE_REVIEW.md. Review the complete diff against base
> 07f054c19d7045281a667e185b9bfc74f3cde29e, including library, tests, generator,
> frozen fixtures and docs. The implemented scope is byte data, all-version
> planning and RS/interleaving only. Matrix/masking/rendering/card work is
> intentionally pending. Identify concrete correctness, state/attribute,
> portability, test-independence or integration defects, with file/line,
> reproducer and user impact. Run relevant tests if available and report the
> commands actually executed. Return the reviewed head SHA, prioritized
> findings and a verdict for this checkpoint. Do not merge or implement later
> feature stages as part of the review.

## Report format

Record reviewed head/base, scope, tests actually run (or not run), findings
ordered by severity with reproduction and location, and a checkpoint verdict:
changes required / no blocking finding. Distinguish current defects from
future-stage requirements. A clean checkpoint review does not approve a merge
or complete the full feature. Fixes should be separate tested commits on this
same feature branch, with the review record linked from the eventual PR.

## Publish through git and gh

Run the [publication script](../../../scripts/publish_qr_pr.sh) from this
checkout in a terminal with network access and authenticated `gh`:

```sh
bash scripts/publish_qr_pr.sh
```

It requires a clean committed feature branch and the expected origin, checks
authentication and existing PRs, pushes without force, then creates the draft
or updates the existing draft's title/body. It prints the URL and head SHA.
It stops on errors and can be rerun after a partial failure. An existing PR
that is already ready for review requires manual inspection before updating.
It does not create issues, commit changes or merge anything.

The PR stays draft for this incomplete feature and the requested Opus review.
After creation, inspect the actual PR checks; no remote green-CI result is
claimed before observing it. The ready-to-file issue queue remains separate;
replace local ticket IDs with real issue links only after publication succeeds.
