---
name: functional-postscript
description: Compose reusable PSCat graphics with array combinators, explicit value capture, and procedure advice. Use when authoring or maintaining Functional PostScript artwork or instrumentation; ordinary rendering alone uses the pscat skill.
---

# Functional PostScript

For agent-authored artwork, separate record data, transforms and drawing marks.
Load `(lib/functional.ps) run`, then `(lib/aspect.ps) run` for instrumentation.
These libraries ship with this feature's PSCat build; released binaries from
before this feature do not include them. No engine extension is needed.

Stack top is on the right; `...` is an untouched prefix. Quotations are `{...}`,
collections are literal arrays `[...]`. Use `/draw load` to pass a named
procedure; `draw` calls it. Call returned quotations with `exec`.

| Call | Effect / callback |
|---|---|
| `array q fp-map` | fresh array; q `( x -- y )` |
| `array q fp-filter` | selected original values; q `( x -- bool )` |
| `array initial q fp-fold` | left fold; q `( acc x -- acc )` |
| `array q fp-each` | ordered effects; q `( x -- )` |
| `p q fp-compose` | quotation executing p then q |
| `value q fp-curry` | quotation pushing captured value then calling q |
| `value q fp-dip` | execute q below value, then restore value |
| `value q fp-keep` | run q on value, then push value again |
| `array q fp-map-indexed` | like map; q `( x i -- y )` |
| `n fp-range` | `[0 ... n-1]` |
| `a b fp-zip` / `a b q fp-map2` | pairs / q `( x y -- z )`; equal lengths |
| `array key fp-sort-by` / `array fp-sort` | stable sort; key `( x -- num/string )` |
| `key fp-field` | getter `( dict -- value )`; use dict records |

Capture config/palettes as data; this is not lexical scope. Names inside user
quotations remain dynamic. Array traversals snapshot slots; nested composites
and captured objects still alias. Callbacks run in your current dictionary
without hidden stack operands. They must preserve the prefix and balance their
own dictionary/graphics state. Net operand depth is checked, not value identity.
Use native loops for tiny fixed patterns and hot inner loops.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/dot { % x rgb --
  gsave aload pop setrgbcolor 50 translate
  newpath 0 0 8 0 360 arc fill grestore
} def
/calls << /Calls 0 >> def
/mark [0.7 0.2 0.5] /dot load fp-curry calls aop-count def
[20 45 70] /mark load fp-each
calls /Calls get == % 3
showpage
```

AOP constructors return quotations: `target advice aop-before` runs advice then
target; `aop-after` runs advice after **normal return only**; `aop-around` gives
advice a target quotation as an extra input (`exec` is proceed).
`target counter aop-count` increments a writable dictionary's nonnegative integer
`/Calls` before entry. `target (label) aop-trace` prints enter/normal-return leave.
Observation advice must preserve actual stack values and graphics state.
No finally, automatic weaving, global operator replacement or rollback.
Bound operators and previously captured targets bypass later name redefinitions.
Use `--profile` for native timing; library advice supplies hooks, not a new clock.

Make records dictionaries with named fields, not positional arrays. For a
mark consuming a record in several steps, keep it: `{/stem load fp-keep blossom}`.
`/name {decorator} aop-wrap-def` re-points a name (or operator) in the
current dictionary only.
Keep `gsave newpath ... grestore` inside marks that should isolate graphics.

On errors, inspect `--pstack-on-error` and `$error /command`; validation guards
are descriptive `/undefined` names. Test callbacks alone with a sentinel.
Do not use `exit` to escape a traversal; filter records instead. `stop` propagates.
Run `--lint-strict` on complete art and exact guard self-tests on libraries.

Read [API](../../../docs/functional-postscript/API.md) for edge cases,
[agent guide](../../../docs/functional-postscript/AGENT-GUIDE.md) for patterns,
[AOP](../../../docs/functional-postscript/AOP.md) for interception limits, and
[implementation](../../../docs/functional-postscript/IMPLEMENTATION.md) before
extending. The executable poster/control are in
[demo](../../../docs/functional-postscript/DEMO.md); the full technical index is
[here](../../../docs/functional-postscript/README.md).

For measured glyph layout, contoured text and a real contract-failure proof, see
[live typography and AOP](../../../docs/functional-postscript/WASM-STUDIO.md).
