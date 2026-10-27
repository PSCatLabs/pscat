# Functional PostScript

An experimental, dependency-free pair of PostScript libraries for PSCat:
seven functional combinators and five explicit procedure decorators. No core
interpreter changes. Intended for reusable, deterministic ArtColony artwork.
The work is on `feature/functional-postscript`, pending independent Opus review.

Start with the [offline illustrated introduction](index.html), or give an agent
the [concise skill](../../plugin/skills/functional-postscript/SKILL.md) and
[agent guide](AGENT-GUIDE.md). All commands below run from the repository root.

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
| [Testing](TESTING.md) | Automated checks, example validation, visual identity and CPU measurements. |
| [Demo](DEMO.md) | Poster source, composition pipeline, colourways and reproduction. |
| [Agent guide](AGENT-GUIDE.md) | Effective patterns, debugging and common mistakes. |
| [Decisions](DESIGN-DECISIONS.md) | Tradeoffs, rejected alternatives and future work. |
| [Engine friction](ENGINE-FRICTION.md) | Prioritized bugs, limits, usability findings and enhancements. |
| [Review handoff](REVIEW-HANDOFF.md) | Independent Claude Opus review entry point. |
| [Progress](PROGRESS.md) | Session checkpoints, commands and validation status. |
| [Initial design](DESIGN.md) | Phase A scope and primary research references. |

The showcased image is procedural graphics rendered by PSCat from ordinary
PostScript; no image model output or third-party runtime code is used.
The libraries retain dynamic name lookup and mutable composite references.
They add neither static types nor lexical closures. Read the API contracts
before composing callbacks with side effects.
