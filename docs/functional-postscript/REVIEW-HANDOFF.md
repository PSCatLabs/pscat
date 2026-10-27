# Independent Claude Opus review handoff

Review `feature/functional-postscript` against its base `ee27313` in this
checkout. Do not merge. Codex implemented the feature; this handoff does not
substitute for your independent correctness and perceptual review.

## Goal and scope

Test whether useful functional and aspect-oriented primitives for agent-created
art can be built using PostScript itself. The result adds ordinary libraries,
a deterministic three-colourway concert poster, a skill, precise Markdown
reference and an offline illustrated HTML showcase. No runtime dependency or
core interpreter change, automatic weaving, lexical closure or static type
system was introduced. User-authorized experimental workflow replaces normal
issue/slice-PR/automated-review orchestration; no main merge is authorized.

## Entry points

- [HTML introduction](index.html): open directly, no server/network needed.
- [Technical documentation index](README.md).
- [API](API.md), [implementation](IMPLEMENTATION.md), [architecture](ARCHITECTURE.md),
  [AOP](AOP.md), [decisions](DESIGN-DECISIONS.md), [agent guide](AGENT-GUIDE.md).
- [Agent skill](../../plugin/skills/functional-postscript/SKILL.md), linked from
  root AGENTS.md. This is an entry point, not a duplicate of the full reference.
- [Engine friction](ENGINE-FRICTION.md), with status/impact/reproduction and
  prioritized review questions. No automatic issues were created.
- [Progress and validation evidence](PROGRESS.md).

## Implementation and API

[lib/functional.ps](../../lib/functional.ps): `fp-map`, `fp-filter`, `fp-fold`,
`fp-each`, `fp-compose`, `fp-curry`, `fp-dip`. Literal-array operations snapshot
slots, preserve order and check callback net depth. Fold uses explicit identity.
Curry boxes arbitrary executable data; compose retains quotation boundaries.

[lib/aspect.ps](../../lib/aspect.ps): `aop-before`, `aop-after`, `aop-around`,
`aop-count`, `aop-trace`. These return captured procedure wrappers. After means
normal return. Around receives target as data; `exec` is proceed. Count mutates
an explicit `/Calls` dictionary before attempted entry. Trace uses normal stdout.
Before/after/around are instrumentation/contract hooks; native profiling supplies
timing rather than a new timer library.

All library callbacks run in the caller's dictionary, with no private operands.
A fresh state frame lives in generated execution code and is opened only
between callbacks; the frame does not refer back to its generated program.
Helpers/operators are captured at load; user quotations are not rebound.
Aliases, dynamic lookup, native errors and graphics state retain ordinary PS
behavior. Guards use PSCat's existing descriptive undefined-name convention.

## Files to inspect

- Both new libraries, their exact-name `%%SelfTest` guards and selftest manifest.
- [tests/functional.rs](../../tests/functional.rs) and
  [semantic corpus](../../tests/data/functional_semantics.ps).
- [Functional poster](../../examples/functional/poster.ps),
  [imperative control](../../examples/functional/imperative.ps),
  [shared drawing vocabulary](../../examples/functional/common.ps).
- [Reproduction/benchmark tool](../../scripts/functional_demo.py),
  [documentation checker](../../scripts/check_functional_docs.py).
- New skill and small links in AGENTS.md, README and guide index.
- No engine implementation files or dependency manifests should differ.

## Commit checkpoints

- `6271862` — architecture/research and experimental workflow.
- `3d54566` — functional combinators, explicit advice, API and semantic tests.
- Later demo/documentation/validation checkpoints are recorded in PROGRESS.md;
  inspect `git log --oneline ee27313..HEAD` for the full final sequence.

## Reproduce and verify

From repository root:

```sh
cargo build --profile ci --locked
cargo test --profile ci --test functional --test capabilities --test skill_blocks
PSCAT_PROFILE=ci ./scripts/selftest.sh
python3 scripts/functional_demo.py render
python3 scripts/check_functional_docs.py --pscat target/ci/pscat
```

Full gate: `cargo test --profile ci`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, Markdown links, gallery manifest and licence checks.
See TESTING.md and PROGRESS.md for actual results and environment limitations.
The existing process-deadline pgrep control requires an unrestricted process
view on this macOS host; do not weaken it because sandboxed visibility fails.

[assets/](assets/) has actual poster/primitive/detail/intermediate PNGs and their
[hash manifest](assets/render-manifest.json). `poster.ps` produces 13 flowers,
radius checksum 622, with enter/leave garden lines. `Style` 0/1/2 changes palette;
`Layer` 1 gives the default ground intermediate. Default seed is 314159.

All three styles have full-buffer equality against their imperative control
and committed PNGs. Independently assess their appearance; equality alone does
not demonstrate artistic quality. Assess desktop/mobile HTML and click the
style/layer and stack controls; these select pre-rendered evidence, not live PS.

## Measurements and limitations

[MEASUREMENTS.json](MEASUREMENTS.json) holds five paired child user+system CPU
samples with source hashes, platform and profile. Map/fold over 1,000 values,
100 repetitions, costs about 5.2× the imperative loop. The small full poster is
similar between drivers. No speedup or general agent-productivity claim.
Driver lines are 19 vs 24 excluding shared helpers and libraries, descriptive only.
Native profiler timing is elapsed wall time; it is not the benchmark clock.

There is no static effect inference, lexical environment, deep immutability,
lazy stream, fused traversal, general pointcut system, finally advice, rollback
or hostile-callback isolation. Depth checks do not detect same-depth prefix
replacement. Captured composite values/procedures alias; names remain dynamic.
Array snapshots/filter capacity cost O(n). Deep compositions can hit execution
limits. Callback-owned dictionaries and graphics saves must be balanced.
Previously captured procedures/bound operators bypass later name replacement.
Trace labels can contain literal newlines. Standard GS access permissions are
not emulated by PSCat, so this API does not rely on readonly enforcement.

## Questions deserving scrutiny

1. Verify native/PLRM executable-array, name/string/operator capture and packed
   quotation semantics, including accidental autoexecution while constructing.
2. Verify nested/currentdict/def correctness, no hidden operand state, no frame
   cycles, and error/stop/exit paths. Challenge same-depth guard limitations.
3. Check all public stack contracts, snapshot/alias semantics and error claims
   against code and tests; check the before/after/around ordering independently.
4. Judge whether the actual poster benefits from explicit composition and
   configuration, rather than counting combinators as success. Compare control.
5. Evaluate library overhead; no dispatch/allocation attribution is established.
6. Assess source/skill/HTML/reference consistency and accessibility/offline use.
7. Prioritize engine findings: clock semantics, diagnostics/effect tooling,
   snapshot memory, anonymous profiler attribution and existing packing/access
   gaps. No engine enhancement was needed to implement the feature.

## Launch the independent review

Give a fresh Claude Opus session this repository/branch and this handoff. Ask it
to assess PostScript correctness, functional/AOP design, compatibility,
simplicity, test coverage, performance, agent usefulness, actual visual benefit,
engine-change justification, and both human/agent documentation. Ask for
severity-ranked findings with source references and reproductions, and a
separate perceptual assessment of the rendered poster/HTML. Keep its review
independent and leave the feature branch active for subsequent fixes.
