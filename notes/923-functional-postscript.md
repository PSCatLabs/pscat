## Functional PostScript libraries: lib/functional.ps and lib/aspect.ps (PR #923, 2026-11-04)

Codex built this as a user-directed experiment on one feature branch
(2026-10-27/28), outside the usual issue-and-slice flow; the design and
progress record is `docs/functional-postscript/` (DESIGN.md, PROGRESS.md).
An independent review merged it for 0.7.0.

What shipped:

- `lib/functional.ps`: `fp-map`, `fp-filter`, `fp-fold`, `fp-each`,
  `fp-compose`, `fp-curry`, `fp-dip`. `lib/aspect.ps`: `aop-before`,
  `aop-after`, `aop-around`, `aop-count`, `aop-trace`. Plain PostScript, no
  engine change; compiled into the binary like every `lib/**/*.ps`, so the
  CLI, `pscat-mcp` and the WASM build all have them. They appear in the
  `--capabilities` catalog and in `docs/guides/functional.md`, which makes
  the twelve procedures Stable under `docs/STABILITY.md`.
- The user guide `docs/guides/functional.md`, and a short plugin skill
  `plugin/skills/functional-postscript/`.
- `examples/functional/`: the Chromatic Bloom poster with an imperative
  control that draws the same pixels, and the typography/observable-press
  studies behind the offline WASM studio in `docs/functional-postscript/`.

Changed in review:

- CI failed because `check_functional_docs.py` hashed every `src/**/*.rs`,
  `Cargo.toml`, `Cargo.lock`, `build.rs` and `lib/artkit.ps` against the
  committed studio bundle, and `tests/functional.rs` compared renders with
  committed PNGs byte for byte. Any unrelated engine or dependency change
  would have turned `main` red. Those staleness checks are now rebuild-time
  only: `check_functional_docs.py --provenance` and two `#[ignore]`d tests
  (`cargo test --profile ci --test functional -- --ignored`). The default
  suite keeps the self-contained checks: functional vs imperative pixels in
  the same run, the gs semantics corpus, guards, payload-vs-manifest, links
  and every doc example executing.
- The committed WASM payload (7.1 MB, repo-only, with its generated
  dependency licence page) and the PNGs are kept as a frozen snapshot; they
  still matched today's renders at merge time.
- Docs restructured: one user guide in `docs/guides/`, linked from the
  guides index, README and when-to-use; the deep notes stay in
  `docs/functional-postscript/` and are linked once, from the guide.

Deferred: the plugin manifest's description still says "Three skills"
(`plugin/plugin.json` is an owner path).
