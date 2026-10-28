# QR issue queue (ready to publish)

GitHub publication is blocked by connector HTTP 403 and an invalid CLI token.
No issue number has been assigned. The owner explicitly authorized this initial
epic/decomposition; once filed, ordinary agents propose follow-ups under the
repository's established dispatcher policy instead of opening unrelated issues.

Branch: `feature/qr-code-library`. [Architecture/sequence](QR_CODE.md).
After publication, replace local ticket IDs below with issue links and record
the epic's dependency checklist. Keep this file while the branch is active.

| ID | Issue title | Depends on |
|---|---|---|
| QR-0 | Epic: native PostScript QR Model 2 and PSCat business card | all below |
| QR-1 | QR: byte-mode packing and independent codeword fixtures | none |
| QR-2A | QR: standards capacity tables and automatic version selection | QR-1 |
| QR-2B | QR: GF(256), Reed–Solomon blocks and interleaving | QR-2A |
| QR-3 | QR: reserved patterns and fixed-mask matrix construction | QR-2B |
| QR-4 | QR: BCH metadata, mask penalties and first decoded URL | QR-3 |
| QR-5 | QR: public vector API, embedded distribution and exporters | QR-4 |
| QR-6 | QR: full version/ECC boundary and independent decode corpus | QR-5 |
| QR-7 | Gallery: procedural two-sided printable PSCat business card | QR-6 |

## QR-0 — epic

Pure PostScript Model 2 versions 1–40, byte mode, automatic version selection,
exact L/M/Q/H, Reed–Solomon, all function/format/version patterns, eight masks
and penalty scoring. Separate matrix/vector API, default four-module quiet
zone, configurable size/location/colors. No QR-specific Rust primitive.
Same procedural card artwork exports SVG/PDF/PNG and decodes to
`https://github.com/PSCatLabs/pscat`. Follow existing distribution/gallery/SDLC
conventions and keep the functional experiment separate. No merge to main in
this initial effort. Complete only after tests, docs and independent review.

## QR-1 — initial independently verifiable slice

- Pack one byte segment into an explicitly supplied version/data capacity.
- Mode/count fields, raw bytes, terminator, byte alignment, EC/11 padding.
- Input is not mutated; fresh output, no stack/frame leaks; exact guard tests.
- Pinned independent fixtures, including empty/binary/URL and boundary cases.
- PSCat behavior, real Ghostscript parity, selftests and capability checks pass.
- This ticket does not claim to produce a QR symbol. Public interface remains
  provisional until QR-5. Preserve provenance and offline normal test execution.

## QR-2A — capacity planning

- Standards tables for versions 1–40/every ECC, independently cross-checked.
- Select smallest fitting version; exact max/max+1 and count-width boundaries.
- Exact requested ECC, with invalid-level/oversize diagnostics and no drawing.
- Feed selected capacity/version into QR-1 and compare independent data vectors.

## QR-2B — error correction

- GF(256) multiply, generator/remainder, splitting and unequal-group interleave.
- Reference parity vectors and full interleaved codewords; bounded resource use.
- Exercise multiple block groups/degrees across all four levels and version 40.
- Plain PostScript only; no new interpreter operator or runtime dependency.

## QR-3 — matrix construction

- Finder/separator, timing, alignment, dark module and metadata reservations.
- Version-dependent alignment centers; zigzag placement skips reserved cells.
- Remainder bits and forced masks; raw matrix compared to fixed-mask oracle.
- Include versions 1, 2, 7, 10, 27 and 40 and unequal-block fixtures.

## QR-4 — masks and working symbol

- All eight mask predicates applied only to data cells.
- Correct format/version BCH placement for each ECC/mask.
- Four penalty rules with targeted edge cases and deterministic tie breaking.
- Minimal vector proof produced by PSCat independently decodes the GitHub URL
  at L/M/Q/H. Do this before decorative or gallery work.

## QR-5 — API/distribution/export

- Implement/document `qrencode`/`qrdraw`, options and matrix ownership/orientation.
- Validate dimensions/colors/quiet zone and preserve caller graphics/path state.
- Run from embedded distribution without checkout; tagged capabilities agree.
- SVG/PDF/PNG preserve matrix/quiet zone; rasterized exports decode the exact URL.
- Child commands have deadlines and unique scratch dirs; diagnostics name fixture,
  version, ECC, mask and failing stage. No required decoder silently skipped.

## QR-6 — conformance regression corpus

- All 40 versions/all four ECC levels at capacity transitions, binary payloads,
  short/long URLs, oversize/invalid options and repeated calls.
- Compare independent codewords and fixed-mask matrices; decode rendered corpus.
- Test SVG/PDF physical dimensions, PNG pixels/print recipe, four-module minimum
  and module boundaries at multiple scales. Run full repository quality gate.
- Record standards references/limitations; final independent cross-family review
  remains required before any future merge.

## QR-7 — gallery card

- Two procedural 252×144 pt sides, polished PSCat typography/branding and clear
  call to action, exact repository QR, no raster images or optional font packs.
- Same source supplies both sides to existing exporters; actual decoded export
  images, physical sizing/safe margins and quiet zones asserted automatically.
- Gallery manifest/renders/generated pages synchronized; card-size proof and
  independent perceptual review meet `docs/dev/TASTE.md` and this acceptance bar.
- Reassess main's available functional primitives at implementation time; do not
  copy/merge unreviewed code. Any optional dependency gets an explicit strategy.
