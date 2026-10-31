# Functional PostScript: design notes

**To use the library, read the guide: [Functional PostScript](../guides/functional.md).**
This folder is the design record behind it, for people and agents who want to
dig: how `lib/functional.ps` and `lib/aspect.ps` were built, the contracts in
full, the test evidence and the engine friction found along the way.

The libraries began as an experiment (Codex-built, 2026-10-27/28) in writing
seven functional combinators and five explicit procedure decorators in plain
PostScript, with no interpreter changes, for reusable, deterministic
agent-made art. An independent review merged them in PR #923 for 0.7.0
(`notes/923-functional-postscript.md`). Pages below that describe a review
still to come, or a branch not yet merged, are the record of that time.

The [illustrated introduction](index.html) and the
[live offline WASM studio](live-studio.html) open straight from a checkout.
The studio runs a WASM build of pscat frozen at the commit that made it
([WASM-STUDIO.md](WASM-STUDIO.md) says how to rebuild it). All commands below
run from the repository root.

```sh
cargo build --profile ci --locked
python3 scripts/functional_demo.py render
# Open docs/functional-postscript/index.html directly in a browser.
```

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
[1 2 3] 10 {add} fp-curry fp-map == % [11 12 13]
/calls << /Calls 0 >> def
/double {2 mul} calls aop-count def
4 double == % 8
calls /Calls get == % 1
```

| Document | Read for |
|---|---|
| [API](API.md) | Every procedure's stack effect, types, errors, aliasing, examples. |
| [Architecture](ARCHITECTURE.md) | Integration boundaries, execution model and data flow. |
| [Implementation](IMPLEMENTATION.md) | Generated quotations, state frames, snapshots and extension rules. |
| [AOP](AOP.md) | Advice ordering, interception, tracing, counting and profiling hooks. |
| [Warp](WARP.md) | Non-affine path warps, path transforms, and running unmodified programs through them. |
| [Testing](TESTING.md) | Automated checks, example validation, visual identity and CPU measurements. |
| [Live WASM studio](WASM-STUDIO.md) | Typography, contracts and actual local browser rendering. |
| [Demo](DEMO.md) | Poster source, composition pipeline, colourways and reproduction. |
| [Agent guide](AGENT-GUIDE.md) | Effective patterns, debugging and common mistakes. |
| [Decisions](DESIGN-DECISIONS.md) | Tradeoffs, rejected alternatives and future work. |
| [Engine friction](ENGINE-FRICTION.md) | Prioritized bugs, limits, usability findings and enhancements. |
| [Review handoff](REVIEW-HANDOFF.md) | The brief for the independent review (historical). |
| [Progress](PROGRESS.md) | Session checkpoints, commands and validation status. |
| [Initial design](DESIGN.md) | Phase A scope and primary research references. |

The showcased image is procedural graphics rendered by PSCat from ordinary
PostScript; no image model output or third-party runtime code is used.
The libraries retain dynamic name lookup and mutable composite references.
They add neither static types nor lexical closures. Read the API contracts
before composing callbacks with side effects.
