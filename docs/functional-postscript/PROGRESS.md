# Progress log

## 2026-10-27 — Phase A

- Cloned PSCat into this checkout; fetched origin; created
  `feature/functional-postscript` from `origin/main` at `ee27313`.
  No pre-existing remote feature branch. No other checkout touched.
- Read repository architecture, lifecycle, library tags, self-test conventions,
  skill examples, relevant history on callback scoping and binding, and the
  underlying execution/collection implementation.
- Researched Joy, Factor, Cat, Adobe PLRM and AspectJ using primary sources.
- Design is in DESIGN.md. No core modifications planned. API is deliberately
  prefixed and array-only; explicit value capture is not lexical closure support.
- Baseline `cargo build --locked`: PASS.
- Next: implement and validate pure-PostScript primitives, decorators, then demo,
  detailed docs, skill, offline HTML and final review handoff.

## Phases B/C — functional and aspect libraries

- Added seven functional procedures and five explicit decorators, with existing
  capability tags and the embedded loader; no Rust engine or dependency changes.
- Traversals use per-call execution captures and shallow slot snapshots.
  Rejected a self-referential frame/body layout to avoid an Rc cycle.
- 23 Rust behavioral tests pass, including native/Ghostscript stdout agreement
  with real packed quotations. Existing capability tests: 21/21 pass.
- Four PS self-test blocks pass; strict-lint rendering driver is clean.
- API.md records type, stack, mutation, errors and limitations for every API.
- Fixed implementation mistakes during development: quotation construction order,
  baseline count including its own key, and executable trace-label retrieval.
  Regression tests pin each relevant behavior.
- Next: finish rendered art comparisons, docs/skill and offline HTML validation.

## Phase D — reproducible visual demonstration

- `examples/functional/` contains the shared drawing vocabulary, functional and
  imperative drivers, primitive sheet, detail and numeric benchmark.
- Rendered seven actual local PNGs under assets/, including three colourways.
  Visually inspected Ultraviolet, Sunprint, Blue Hour, vocabulary and detail.
- Native regression test compares all three full RGBA buffers with imperative
  controls and committed PNGs. Default trace/count/checksum: garden / 13 / 622.
- New behavioral suite now 24 tests, all pass. Existing full `cargo test
  --profile ci` passed outside sandbox; sandbox-only first failure was an
  existing pgrep visibility control, not a functional library failure.
- Paired five-sample CPU data in MEASUREMENTS.json identifies input hashes and
  build/platform. Numeric safety/snapshot overhead ~5.2x; poster medians similar.
  Driver non-comment lines: 19 functional, 24 imperative, excluding shared code.
- `scripts/functional_demo.py render` regenerates art and hash manifest;
  `benchmark` records raw samples and synchronizes the HTML's rounded medians.
- Full clippy passed; all library selftests and strict rendering drivers passed.
