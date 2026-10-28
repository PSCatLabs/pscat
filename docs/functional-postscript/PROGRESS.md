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

## Phases E/F/G — agent adoption and documentation

- Concise skill under plugin/skills/functional-postscript; AGENTS.md explicitly
  directs agents there. Skill blocks run on a bare embedded-library install.
- Added full technical index, architecture/API/implementation/AOP/testing/demo/
  agent/decision documents, engine findings and independent review handoff.
- Offline index.html uses actual renders, inline CSS/JS, a stack walkthrough,
  palette/layer controls, side-by-side controls and raw measured evidence.
- Chrome 156 checked file:// at desktop 1440×1000 and mobile 390×844: all
  images loaded, no horizontal overflow, no JS exceptions, no HTTP(S) requests.
  Palette, intermediate-layer, step and reset controls passed. Screenshots were
  visually inspected; paths/checks are in BROWSER-VALIDATION.json.
- Standard-library checker validates 29 HTML local targets, 9 image elements,
  seven PNG hashes/sizes, four metric cells, input measurement hashes and 14
  complete Markdown/skill PostScript examples. Added as a 25th Rust test.
- The optional system skill-creator quick_validate.py could not import PyYAML.
  No dependency was installed. The project checker validates this skill's
  simple frontmatter/name/description/placeholders, links and executable block.
- Engine findings distinguish clock semantics, measured traversal overhead,
  standard PS capture/scope hazards, diagnostics and hypothetical platform work.
  No core changes or automatically filed issues.
- Remaining: staged-file link/licence checks, final full gate, clean commit
  checkpoints and feature-branch review availability.

## Phase H — final validation, ready for independent review

Checkpoints: `6271862` design; `3d54566` libraries; `4e9201b` demonstration;
`cf2cfeb` HTML, skill, technical reference and documentation checker.
The closing handoff commit follows them in `git log ee27313..HEAD`.

Final local gate (2026-10-27):

- `cargo build --locked`: PASS.
- `cargo test --profile ci`: 2,532 passed, 2 existing ignored, 0 failed;
  113 result groups, including all 25 new functional/documentation tests.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `PSCAT_PROFILE=ci ./scripts/selftest.sh`: all blocks/drivers PASS.
- `python3 scripts/check_md_links.py`: 107 files, zero broken links.
- `python3 scripts/gallery.py check`: 34 pieces / 37 specimens, PASS.
- `python3 scripts/check_licences.py all`: PASS.
- Default `python3 scripts/functional_demo.py render`: PASS, all PNGs reproduce
  the committed hashes. Explicit --pscat and benchmark routes were also run.
- `python3 scripts/check_functional_docs.py --pscat target/ci/pscat`: PASS;
  14 executable examples, 29 HTML local targets, 9 image elements, seven asset
  hashes/sizes, four metrics and source hashes. Skill frontmatter checked.
- Chrome desktop/mobile checks and visual inspection: PASS; details in
  BROWSER-VALIDATION.json. Browser closed after validation.
- Native `--profile --profile-graph` demo: PASS; named df-flower/pkribbon each
  counted 13, df-site 25. Timing is elapsed, not CPU.
- No changed `src/`, Cargo.toml or Cargo.lock. No dependency added. No main
  changes/merge, issues, PR orchestration or automated reviewer invoked.

Full gate logs are session scratch files in /tmp/functional-final-*.log and
/tmp/functional-selftests.log; committed tests, hash manifests and browser report
are the durable evidence. Sandbox process visibility required the full-suite
rerun outside the sandbox; no test was weakened. Optional system skill validator
lacked PyYAML; independent standard-library checks cover the actual skill.

Implementation and requested deliverables are complete. Independent Claude
Opus review remains deliberately outstanding. Start with REVIEW-HANDOFF.md;
keep this branch active and record any subsequent reviewer findings/fixes here.
