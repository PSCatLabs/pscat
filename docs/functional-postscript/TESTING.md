# Testing and measurements

Tests exercise the shipped libraries through PSCat, not a second implementation.
No engine source or dependency changes were made. Full validation status and
checkpoint hashes are recorded in [PROGRESS](PROGRESS.md) and the
[review handoff](REVIEW-HANDOFF.md).

## Automated coverage

- [tests/functional.rs](../../tests/functional.rs): 29 behavioral and documentation tests. Empty,
  singleton and nested arrays; order and noncommutative fold; composition
  boundaries/associativity; arbitrary executable captured data; dictionary
  lifetime vs dynamic lookup; caller `count`/`currentdict`/`def`; nesting and
  reentry; source snapshot and shared aliases; procedure mutation; errors and
  stop; large nonrecursive traversal; nested decorators; proceed/skip/repeat;
  attempted-entry counting; bound vs dynamic operator interception; helper
  shadowing/reload; captured stdout; graphics/pixel preservation; three-colourway
  imperative/control/golden pixel identity. See exact test names in source.
- [tests/data/functional_semantics.ps](../../tests/data/functional_semantics.ps):
  corpus whose stdout is compared with real Ghostscript, with
  `true setpacking` to exercise genuine packed quotations. If gs is absent the
  Rust test prints a skip; local validation used installed gs. This is semantic
  agreement on the corpus, not proof of universal interpreter compatibility.
- Three additional study tests check measured glyph advances/page-space bounds,
  eight saved typography/AOP buffers, unchanged target pixels at the **same**
  coordinates, fault-disable/changed-word counts, and full native/WASM equality
  for 15 representative configurations from the exact committed offline bundle. Translated proof
  rows can differ by one anti-alias LSB due to coordinate precision, so they
  are not used as a same-pixel-position control.
- A font-layout regression checks five short/varied words for all eight
  bundled faces: width/height bounds, italic sidebearings and retention of the
  captured font even if TypeFont changes later. The WASM script also exercises
  all 48 font/treatment combinations and all eight proof fonts without warnings.
- Four `%%SelfTest` blocks next to guards in
  [functional.ps](../../lib/functional.ps) and [aspect.ps](../../lib/aspect.ps)
  assert exact named guards, including wrong container/quotation types,
  callback result counts, predicate boolean, early exit, counter and trace
  contracts. The self-test harness itself rejects stack/save leaks.
- [selftest/drivers/functional.ps](../../selftest/drivers/functional.ps): strict
  lint of a rendered each/count composition. Existing capability tests discover
  both new libraries through their tags and validate every public/internal name.
- Existing `tests/skill_blocks.rs` discovers the new skill's runnable PS block
  and runs it on a bare embedded-library install with no font packs.
- [check_functional_docs.py](../../scripts/check_functional_docs.py): local HTML
  anchors/assets, PNG dimensions/hashes, metric/source-hash consistency and
  complete Markdown/skill PostScript examples. The Rust integration test runs
  this checker with its own PSCat binary. Browser layout/interactivity are
  separately checked in headless Chrome; no browser dependency is added to CI.
  The checker also verifies all fifteen image hashes and decompressed WASM/API,
  example and engine-source provenance. `scripts/check_functional_studio.mjs`
  executes the exact bundle under Node without fetching or compiling WASM.

## Commands

```sh
cargo build --locked
cargo test --profile ci
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
PSCAT_PROFILE=ci ./scripts/selftest.sh
python3 scripts/check_md_links.py
python3 scripts/gallery.py check
python3 scripts/check_licences.py all
python3 scripts/check_functional_docs.py --pscat target/ci/pscat
```

Targeted iteration:

```sh
cargo test --profile ci --test functional --test capabilities --test skill_blocks
target/ci/pscat --selftest lib/functional.ps
target/ci/pscat --selftest lib/aspect.ps
```

Tests that spawn children use `common::CommandDeadline`; any test scratch file
uses `common::scratch_dir`. No wall-time assertion or new timing threshold is
introduced. On this macOS host the existing `child_deadline` pgrep control
cannot see its child inside the filesystem/process sandbox; the full suite is
run outside that sandbox rather than weakening the test.

## Visual evidence

`poster_variations_match_the_imperative_control_and_committed_renders` compares
complete RGBA buffers between drivers at 720×960 and committed colourway PNGs.
It also verifies stack cleanup and the printed 13/622 invariants, and asserts
all three variants differ. This catches orchestration differences and asset
drift. It does not replace inspecting the poster for legibility/composition.
The default poster, all colourways, primitive sheet and detail were visually
inspected during implementation; independent perceptual assessment remains for
Opus. Rerender with `python3 scripts/functional_demo.py render` only for an
intentional image change, inspect it, then update/commit expected evidence.

## Performance method and observations

```sh
python3 scripts/functional_demo.py benchmark --pscat target/ci/pscat
```

The standard-library Python script runs five paired child processes per
workload, sums each child's user + system CPU using `resource.getrusage`, and
records every sample, median and minimum in [MEASUREMENTS.json](MEASUREMENTS.json).
It includes process startup/library loading and uses the optimized ci profile
with debug assertions and overflow checks. It records platform, binary version,
source checkpoint and hashes of the actual inputs. The source checkpoint may
precede committing the demo; source hashes identify those files precisely.
`--pscat` can select another build, so record its profile honestly if comparing.
Benchmarking requires Unix `resource`; rendering and the libraries do not.

The numeric workload maps squares of 1,000 integers and folds their sum 100
times. Both paths allocate a mapped array and print 332833500; the functional
path additionally snapshots and checks each callback. Observed median CPU is
roughly 5.4× the imperative control. The full poster timings are similar at
the measured scale; no performance advantage is claimed. The HTML displays
rounded medians from the raw report. Tiny differences between poster medians
are not treated as significant or generalizable. Frequency/load, platform,
allocator, rendering complexity and callback cost all matter.

The benchmark updates the report and HTML metric cells, not a pass/fail budget.
It is not part of CI timing assertions. `--profile`/`--profile-graph` can inspect
named procedure elapsed wall times; they measure a different clock and need
not agree with child CPU. AOP counters are deterministic, but timing is not.
See [ENGINE-FRICTION](ENGINE-FRICTION.md) for clock and attribution limitations.

## Adding tests

Put validation guard tests in the library's nearest `%%SelfTest` block and
update `selftest/libraries.txt` only if adding a new library. Use Rust for
pixel readback or real-gs comparisons. A new combinator needs empty, nested,
repeated, wrong-type and callback-error cases; check sentinel values and caller
dictionary identity, not just net depth. Never use a fixed `/tmp` artifact in
Rust integration tests. If adding a CPU budget, use the existing thread-CPU
helpers and the negative budget-scale check specified in AGENTS.md.


## Live studio browser checks

Start an isolated Chrome with a local debugging port, then run:

```sh
node scripts/check_functional_browser.mjs 9338
```

The optional standard-library CDP script verifies actual WASM execution over
file://, all six treatments and eight fonts, text changes, diagnostic/fault toggles, changed-word counts,
invalid-input rejection, watch/stop/restart, actual PNG download and desktop/mobile
layout. It records current page-source hashes and results in
[LIVE-BROWSER-VALIDATION.json](LIVE-BROWSER-VALIDATION.json). This supplements the
original [saved-view browser report](BROWSER-VALIDATION.json). Neither browser
is a new runtime dependency or a CI prerequisite. Source/display paths and the
7.1 MB bundled-runtime tradeoff are documented in [WASM-STUDIO](WASM-STUDIO.md).
