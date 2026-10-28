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

## 2026-10-28 — new typography, visible AOP and a real WASM studio

User requested more ambitious demos, specifically text and AOP, then a live
WASM HTML example. Independent Opus review is explicitly deferred until later;
no reviewer was invoked. Work stays on the existing long-running branch.

- Added `type-common.ps` and `type-terrain.ps`: measured character advances,
  fold-based run layout, configured glyph quotations, real charpath clipping,
  contoured gradients/extrusions, two styles and actual 15-glyph observation.
- Added `observable-press.ps`: the same target appears direct/decorated;
  before validates, after counts returns, around optionally draws ink/advance
  diagnostics. One isolated invalid record yields actual 7/6/1 metrics and an
  exact named error. Word changes/fault disabling update real counts.
- Fixed a demo bounds-coordinate error and used `load` to retrieve the captured
  executable error name. These are standard PS hazards, not new core defects.
- Four new PNGs join the render manifest (11 total). All strict-lint renders pass.
- `live-studio.html` runs the existing PSCat WASM over file://. The generated
  7.1 MB local script contains compressed module bytes, the unchanged JS API and
  actual inlined example sources. No fetch/server/CDN/framework/dependency added.
- `build_functional_studio.py` records payload/API/example/library/engine hashes.
  The bundle's existing engine/font licence terms and generated Rust dependency
  notices are preserved; the new bundle path is classified in licenses/assets.json.
- `studio.js` configures bounded ASCII words, steps actual PS, shows real stdout,
  downloads actual canvas PNGs, and supports watch/stop/restart. Browser elapsed
  timing is explicitly labeled; no PS clock/CPU claims.
- Chrome 156 file:// checks: text/palette changes, 7/6/1 vs 6/6/0 vs 6/5/1
  counts, diagnostic overlay toggle, invalid input, stop/restart, PNG download,
  desktop/mobile layout, no JS exceptions or HTTP(S) page requests. Screenshots
  inspected; durable report is LIVE-BROWSER-VALIDATION.json.
- New targeted suite: 28 tests PASS, including three new tests for metric/bounds
  layout, four saved images and same-coordinate target identity, and full
  native/WASM pixel equality for three actual bundled programs.
- A first test comparing glyphs at different translated baselines saw a 1-LSB
  anti-alias difference; replaced it with a same-coordinate direct/decorated
  control. No rendering test was loosened to conceal an observation difference.
- Original poster benchmark inputs/raw measurements remain unchanged.
- Remaining: final full gate, tracked link/licence checks, checkpoint commit,
  updated review handoff and push of this feature branch only.

### Validation complete for the live studies

- `cargo build --locked`: PASS.
- `cargo test --profile ci`: 2,535 passed, 2 existing ignored, 0 failed,
  across 113 result groups. Feature suite: all 28 PASS.
- Clippy with `-D warnings`, formatting and all PS selftests/drivers: PASS.
- Tracked Markdown: 108 files, zero broken links; gallery and licences: PASS.
- Offline HTML/provenance checker: 50 local targets, 12 image elements, 11 image
  hashes, unchanged API, decompressed WASM and matching source/engine hashes.
- Node executes the exact committed payload for three examples; native/WASM
  full-buffer equality and real output counters pass.
- Browser-tested UI hashes still match current HTML/JS/CSS. The already
  downloaded actual PNG was independently checked at 1280x960. The window was
  closed after the successful browser run; the interrupted optional repeat was
  not necessary and was not counted as a second run.
- Normalized only generated licence line endings/trailing whitespace for the
  clean diff check. A repeatable normalization command is in WASM-STUDIO.md.
- Logs: /tmp/functional-studies-{build,full-suite,clippy,selftests}.log.
- No changes to src/, Cargo manifests/lock or web/pscat.js. No dependency added.
- Ready for the checkpoint commit and feature-only push; Opus review remains
  deliberately outstanding.

### Live studies checkpoint

`6593697` commits the typography/AOP sources, four rendered outputs, real offline
WASM studio, unchanged-API bundle/provenance tooling, licence records, all 28
feature tests and documentation. The closing validation-note commit follows it.

The active human entry point is `docs/functional-postscript/live-studio.html`;
`#press` selects the AOP study directly. Both are linked from index.html.
The feature is complete and validated; commit/push remains confined to
`feature/functional-postscript`. Opus review is intentionally left for later.

## 2026-10-28 — six treatments and eight bundled typefaces

User requested additional live-print styles and font selection. The existing
feature branch and deferred independent-review workflow remain in force.

- Added Blueprint (engraved grid), Risograph (dot screen/teal overprint), Neon
  (layered outline strokes), and Op Art (optical waves), alongside Nocturne and
  Sunprint. Treatment and font selectors rerender automatically through WASM.
- Shared font selection works in both typography and AOP proof: Helvetica Bold/
  Bold Oblique, Times Bold/Bold Italic, Courier Bold, Palatino Bold, Bookman Demi,
  and Avant Garde Demi. All are bundled outline faces; no downloads/dependencies.
- Glyph records now capture the font name as a sixth field. Outlines, measured
  advances and AOP ink bounds all use that stored font. Later TypeFont changes
  cannot silently change an existing run.
- Layout now measures ink extents as well as advances, handles negative italic
  sidebearings, fits both width and height, and centers height-limited short
  words. The typography/proof height caps are 210/170 user units respectively.
- Rebuilt the local runtime source bundle; no engine/API/module code change.
  Regenerated the saved typography examples and added four representative
  style/font PNGs; 15 total images are in the render manifest.
- Targeted suite: 29 PASS. Font-layout tests cover I/J/Q/WIDE/MWMW for all eight
  faces; 15 representative full buffers match native and WASM. All 48 font/style
  combinations and all eight AOP proof fonts execute without substitution notes.
- Chrome file:// validation passed six distinct treatments, eight font choices,
  automatic rerendering, text/diagnostic/fault controls, stop/restart, PNG export,
  desktop/mobile layout, zero JS exceptions and zero page HTTP(S) requests.
  Screenshots inspected; current UI hashes match LIVE-BROWSER-VALIDATION.json.
- Disk pressure interrupted one test link (rust-objcopy reported no space and
  left an incomplete executable). Cleared only this checkout's generated dev/
  release caches and rebuilt the test successfully; no source or user files
  were removed and no test was weakened. Remaining checks use the existing ci
  cache to avoid regenerating large debug artifacts on the constrained disk.
- Remaining: full gate, tracked link/licence checks, commit and feature-only push.

### Reboot recovery

The Mac mini reboot interrupted the closing validation/commit step. All staged
source, image, bundle and documentation changes survived; HEAD remained
`e8d6cf2`. Temporary validation logs did not survive. The saved browser report
still matches all four current UI source hashes, so that completed browser run
is retained rather than represented as a new run.

After recovery, disk space was available again. The first full-suite retry found
cached incomplete executables left by the earlier disk failure (`drykit` could
not execute). `cargo clean -p pscat --profile ci` removed only this checkout's
generated package artifacts (713 files, 1.2 GiB); dependencies and source were
retained. Rebuilding and running the gate with `-j 1` avoids the previous peak
parallel-link disk use. This is a build-cache failure, not an interpreter defect.

Offline documentation/provenance checks, all 14 executable Markdown/skill
examples, 108 tracked Markdown files, gallery and licence checks pass after
the reboot. Final build/suite/selftest results follow below.

### Final validation after recovery

- `cargo build --profile ci --locked -j 1`: PASS.
- `cargo test --profile ci -j 1`: **2,536 passed, 2 existing ignored, 0 failed**
  across 113 result groups, with no compiler warnings. All 29 feature tests pass,
  including the bundled WASM matrix and 15 native/WASM pixel comparisons.
- `cargo clippy --profile ci --all-targets -j 1 -- -D warnings`,
  `cargo fmt --all -- --check`, and
  `PSCAT_PROFILE=ci CARGO_BUILD_JOBS=1 ./scripts/selftest.sh`: PASS.
- Offline checker: 50 local targets, 12 image elements, 15 render hashes,
  four metric cells, actual payload/provenance and 14 executable examples PASS.
- 108 tracked Markdown files: zero broken links; gallery and licences PASS.
  Existing browser-report UI hashes match; four new saved artwork previews
  inspected again after recovery. No additional browser run is claimed.
- Logs: `/tmp/functional-fonts-{build,full-suite,clippy,selftests}.log`.
- Remote fetch confirmed the feature branch had no divergent commits. Main has
  advanced independently; no main merge/rebase or other checkout was touched.
- No changes to core Rust, dependency manifests or the browser API. Independent
  Opus review remains deliberately outstanding.
