# Engine friction and enhancement findings

No core interpreter change was needed. No new crash, stack corruption or
blocking graphics defect was found. Findings below distinguish confirmed
engine/spec differences, standard PostScript limitations, library tradeoffs
and unmeasured enhancement ideas. They are observations for Opus review;
no issues have been filed automatically.

## A. Engine bugs / specification differences

### A1. `usertime` uses elapsed wall time, not processor time

- **Impact:** medium for profiling correctness; not blocking artwork.
- **Classification:** confirmed existing specification difference, by source
  inspection; not a newly introduced regression. No runtime sleep probe was
  added to the test suite.
- **Reproduction:** in a Rust host, construct `Interp`, evaluate `usertime`,
  suspend that host thread for 200 ms without running PS, then evaluate
  `usertime` again. The current source implies a roughly 200 ms increase even
  though interpreter CPU consumption during suspension is negligible.
- **Expected:** PLRM `usertime` measures execution/processor milliseconds.
- **Actual:** `src/ops/level2.rs::usertime` calls `Clock::elapsed_ms`;
  `src/clock.rs` uses `Instant::elapsed`. Its module comment explicitly calls
  both timers wall clocks. On wasm, timers return zero by design.
- **Evidence:** direct source audit; the external benchmark uses child user +
  system CPU instead. No claim that native profiler elapsed time is CPU time.
- **Suggested improvement:** separately expose a real CPU clock and document
  migration of `usertime`; preserve native elapsed profiler behavior deliberately.
- **Core change required:** yes for timer semantics/host integration. Deferred
  because clock repair is unrelated to the functional API and not blocking.

## B. Performance bottlenecks and library costs

### B1. Guarded traversals cost about 5.2× in the map/fold microbenchmark

- **Impact:** high if used in tight numeric inner loops; small in this poster.
- **Classification/status:** measured library overhead, not an identified engine
  bug or a measurement of a single engine subsystem.
- **Reproduction:** `python3 scripts/functional_demo.py benchmark` runs 100 passes
  over 1,000 values, allocating a mapped result then folding it, and pairs five
  functional/imperative trials. It also renders both poster drivers.
- **Expected:** extra snapshots, state dictionary operations, quotation dispatch
  and depth checks cost more than an unchecked purpose-built loop.
- **Actual/evidence:** [raw samples](MEASUREMENTS.json), including platform,
  build, medians and source hashes. Both numeric paths print 332833500. Poster
  paths both print 13/622 and have pixel-identity coverage.
- **Relevant source:** `lib/functional.ps::fp--walk`, `fp--prepare`,
  `fp--collect`; `examples/functional/benchmark.ps`; execution in
  `src/interp.rs` and dictionary access in `src/ops/dict.rs`.
- **Suggested improvement:** retain imperative/native loops for hot paths;
  investigate optional fusion or cheaper frame access only after attribution.
  Dispatch vs lookup vs allocation has **not** been separately measured.
- **Core change required:** no for library fusion; possibly for a separately
  justified dispatch optimization. No native combinators are proposed now.

### B2. Snapshot and filter capacity consume O(n) storage

- **Impact:** medium for large sparse selections.
- **Classification/status:** confirmed library design cost from algorithm
  inspection; heap bytes have not been measured independently.
- **Reproduction:** filter a large literal array with `{pop false}`. The returned
  result is empty but its fresh output's backing array was allocated at n slots;
  `getinterval` returns a view, not compact storage.
- **Expected/actual:** stable visitation even when callbacks edit source slots;
  private snapshot plus output capacity. The API promises freshness, not compact
  backing allocation. Empty result does not retain source values.
- **Relevant source/tests:** `fp--walk`, `fp--finish`, `src/object.rs::PsArray`;
  `input_slots_are_snapshotted_but_nested_objects_alias`.
- **Suggested improvement:** measure large-collection heap use; consider a final
  compact copy for sparse results against its additional allocation/copy cost.
- **Core change required:** no. Deferred until artwork demonstrates the need.

### B3. An offline file-URL WASM demo carries the whole engine/font payload

- **Impact:** medium download/repository size; does not change render semantics.
- **Classification/status:** measured packaging cost, not a new runtime dependency.
- **Reproduction:** `python3 scripts/build_functional_studio.py` compresses the
  existing WASM module and bundles its unchanged API and example sources.
- **Expected/actual:** about 11.5 MB raw WASM and 7.1 MB generated JS; precise
  sizes/hashes are in `assets/studio-manifest.json`. The original picture-only
  page does not execute this bundle.
- **Relevant source:** existing `scripts/build_wasm.sh`, `web/pscat.js`, new
  `scripts/build_functional_studio.py`. Real file:// execution and no page network
  requests were checked in Chrome; native/WASM full-buffer equality is tested.
- **Suggested improvement:** optional smaller font/library bundles could reduce
  weight, but need explicit feature/packaging design. No size optimization was
  attempted or performance benefit claimed.
- **Core change required:** no for the current bundled demo; optional build
  configuration would be a separate platform effort.

## C. Usability friction / standard-language hazards

### C1. Executable values must be boxed when captured

- **Impact:** high correctness risk for agent-written constructors.
- **Classification/status:** confirmed standard PostScript execution behavior,
  not a PSCat bug. The library implements safe capture.
- **Reproduction:** `[(never_defined) cvx] cvx exec` attempts to execute the
  string, raising undefined, rather than pushing it as data.
- **Expected/actual:** procedure-element dispatch executes executable strings
  and names. Executable nested arrays have different encounter semantics.
- **Relevant source/tests:** `src/interp.rs::execute_element`, `fp-curry`;
  `curry_and_dip_preserve_executable_data_without_running_it`,
  `trace_treats_executable_string_labels_as_data`, real gs corpus.
- **Suggested improvement:** use the supplied curry constructor and show
  explicit `/name load` examples in skills. Better debugger renderings could
  expose executable attributes more clearly.
- **Core change required:** no; changing dispatch would break standard semantics.

### C2. Scratch dictionaries silently redirect caller `def`

- **Impact:** high for nested higher-order library code.
- **Classification/status:** confirmed standard dynamic scoping, already noted
  in prior artkit work; not a newly found engine bug.
- **Reproduction:** `/n 0 def 1 dict begin { /n n 1 add def } exec end n`
  leaves outer n at zero. A library opening that dict across a callback would
  silently hide the user's counter update.
- **Expected/actual:** `def` writes currentdict, not a lexical definition site.
- **Relevant source/tests:** dictionary stack in `src/interp.rs`,
  `callbacks_see_exact_caller_stack_and_dictionary`, nested/repeated tests;
  historical gasket/carpet/noise notes in `docs/dev/HANDOFF.md`.
- **Suggested improvement:** retain execution captures with frame closed during
  callbacks; improve agent guidance, not the language's scope rules.
- **Core change required:** no. This implementation avoids the hazard explicitly.

### C3. Guard errors lack precise library-definition source locations

- **Impact:** medium for debugging agent-generated call chains.
- **Classification/status:** confirmed existing diagnostic limitation and library
  convention. Descriptive undefined guards are intentionally recognizable.
- **Reproduction:** `[1] {dup} fp-map` reports `fp-callback-stack-effect`, with
  top-level call-site line attribution rather than a library stack trace.
- **Expected:** actionable public operation, callback contract and source site.
- **Actual:** `$error /command` identifies the guard; source attribution tracks
  the most recent top-level token. Native operator binding can also retain the
  preceding offending-name attribution. No Object source spans are stored.
- **Relevant source:** `src/interp.rs::error_report`, `src/selftest.rs`;
  exact-name guards in both new libraries.
- **Suggested improvement:** optional source-aware execution diagnostics and
  explicit effect-contract metadata. Preserve existing error identity.
- **Core change required:** yes for spans/stack trace; no for clearer docs/tests.

### C4. Depth checks cannot prove stack-prefix preservation

- **Impact:** medium; guards are useful but incomplete.
- **Classification/status:** confirmed library/runtime-check limitation.
- **Reproduction:** `77 [1] {exch pop 99 exch} fp-map` consumes the caller's
  77, replaces it with 99, and returns `[1]`. Net depth is valid.
- **Expected/actual:** depth checking accepts it, as documented; the library
  cannot infer value identity or arbitrary alias side effects.
- **Relevant source:** `fp--collect`; callback-contract tests use actual
  sentinels and `currentdict`, not only stack length.
- **Suggested improvement:** optional agent linting from declared stack effects
  plus tests of representative values. Do not call these checks a type system.
- **Core change required:** no for an external checker; full isolation would be
  a separate language/runtime project and is outside scope.

### C5. Ink bounds are reported in the current user coordinate system

- **Impact:** medium for diagnostic overlay correctness.
- **Classification/status:** confirmed standard PS behavior; a demo bug was
  fixed, not an interpreter defect.
- **Reproduction:** translate a local glyph origin, construct charpath, and read
  pathbbox. The values remain local user coordinates, not global page positions.
- **Expected/actual:** pathbbox obeys the current CTM; treating its result as page
  coordinates after grestore places diagnostic boxes at the wrong origin.
- **Relevant source/tests:** `ty-bounds` in `examples/functional/type-common.ps`
  explicitly adds the record baseline to local ink bounds;
  `typography_layout_measures_advances_and_reports_bounds_in_page_space`.
- **Suggested improvement:** keep coordinate-space contracts in mark/diagnostic
  APIs and verify actual overlay positions. Do not change pathbbox semantics.
- **Core change required:** no.

### C6. Normal URL-fetch WASM loading does not make a file:// demo portable

- **Impact:** medium for readers opening documentation directly.
- **Classification/status:** confirmed browser origin/loading constraint, not a
  PSCat engine bug. The existing Pscat API already offers instantiate(bytes).
- **Reproduction:** normal `Pscat.load()` calls fetch for a `.wasm` URL; local
  file fetch is restricted by browsers. A server normally avoids that constraint.
- **Expected improvement:** open a self-contained local studio with no server.
- **Actual solution/evidence:** a classic local JS script carries compressed
  module bytes and source; native decompression plus a Blob module loads the
  unchanged API and calls instantiate(bytes). Chrome checks show genuine
  rendering, actual stdout and no HTTP(S) requests. Modern browser APIs are
  required; saved previews remain available when unsupported.
- **Relevant source:** `scripts/build_functional_studio.py`, studio.js and
  `LIVE-BROWSER-VALIDATION.json`; detailed rebuilds in WASM-STUDIO.md.
- **Core change required:** no. This is packaging around an existing supported API.

## D. General platform enhancements

### D1. Optional stack-effect tooling

- **Impact:** potentially high for agent reliability; hypothetical, not measured.
- **Reproduction/context:** arbitrary dynamically named quotations can have
  unknown effects; current net-depth guards operate only after normal return.
- **Expected improvement:** recognize declared `( ... x -- ... y )` contracts
  and infer a useful conservative subset for literal/compose/curry quotations.
- **Current behavior:** comments are documentation only. Factor/Cat demonstrate
  conceptual approaches, but their type systems are not present here.
- **Relevant source:** library `% @example` metadata, build.rs, `src/lint.rs`.
- **Validation:** future work would need a corpus of accepted/rejected programs
  and explicit escape hatches; no implementation or benefit measurement yet.
- **Core change required:** not necessarily; source-level tooling can start outside
  the engine. Avoid silently rejecting valid dynamic PS.

### D2. Named provenance for captured procedures in profiler output

- **Impact:** medium for higher-order program observability; hypothetical extension.
- **Reproduction:** capture `/target load` in a wrapper, invoke via exec with
  `--profile`. The wrapper name is visible; the captured target alone is not
  credited as a named invocation. Named calls *inside* its body still appear.
- **Expected improvement:** optional construction/provenance labels for anonymous
  callbacks without changing procedure identity or semantics.
- **Current behavior/evidence:** `src/profile.rs` and `src/interp.rs` use the name
  a procedure is reached through, with NO_NAME for exec. The demo's `df-flower`
  and `df-stem` calls remain named and profileable.
- **Suggested improvement:** investigate an opt-in label/profiling API or source
  spans, not a mandatory wrapper registry.
- **Core change required:** likely for native attribution; no for explicit named
  wrappers at important artistic boundaries.

### D3. Packed arrays and access restrictions are not modeled fully

- **Impact:** low for these libraries; medium for a future immutable API.
- **Classification/status:** confirmed existing missing PSCat functionality,
  documented in core handoff; not a newly introduced bug.
- **Reproduction:** `true setpacking {} type` remains arraytype in PSCat but
  becomes packedarraytype in gs. `readonly`/`executeonly` currently do not enforce
  permissions in PSCat.
- **Expected/actual:** standard PS distinguishes these representations/access
  rules. This library accepts both real quotation types and does not rely on
  read-only wrappers for safety.
- **Relevant source/tests:** `src/ops/level2.rs`, access operators,
  `ghostscript_matches_the_semantics_corpus_including_packed_quotations`.
- **Suggested improvement:** retain gs packed-procedure coverage and document
  shallow, mutable captures. Implement permissions only as a dedicated platform
  effort with VM/access regression tests.
- **Core change required:** yes for packing/access fidelity, neither is required
  for this feature.

## Priority for independent review

1. Verify callback scope, executable capture and absence of frame cycles (C1/C2).
2. Check honest CPU/timing terminology and clock finding A1; separate future fix.
3. Evaluate traversal costs B1/B2 against useful agent-art workloads before
   proposing optimization or native operators.
4. Assess effect tooling and diagnostics C3/C4/D1; these could improve agent
   outcomes without changing PS semantics.
5. Defer profiler provenance and packed/access implementation unless a concrete
   follow-up use case justifies them. No core enhancement blocks this library.
