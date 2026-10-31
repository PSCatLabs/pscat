# Guide for ArtColony and coding agents

Start with the [skill](../../plugin/skills/functional-postscript/SKILL.md), then
read [API](API.md) when a contract matters. This is normal PostScript with a
small higher-order library. Do not invent lexical scope or port another
language's syntax.

## Choose the right shape

Use map for independent record transforms, filter for selection, fold for
checksums/bounds/totals, each for ordered drawing. Keep geometry data separate
from marks. Compose quotations when an intermediate stack contract is useful;
capture palettes/configuration explicitly with curry. Native loops are clearer
for a tiny fixed petal count, simple counters, byte streams or hot inner loops.
The guards and snapshots are overhead: they buy semantics, not speed.

Write a stack comment for every artistic procedure. A useful drawing callback
is `(record palette -- )`. Configure it once as `palette {draw} fp-curry` and
pass that quotation to each. Keep its graphics state balanced; use `newpath`
inside the drawing wrapper if it must not paint an ambient path.

```postscript
(lib/functional.ps) run
/libmark { % x rgb --
  gsave aload pop setrgbcolor 30 translate
  newpath 0 0 8 0 360 arc fill grestore
} def
% One palette value is reused; x is the varying collection input.
[20 45 70] [0.3 0.6 0.8] /libmark load fp-curry fp-each
showpage
```

The example works on a 100x100 page. A more complex mark can load PaintKit
normally. Do not put temporary dictionaries around someone else's callback.
If *your* drawing procedure owns a local dictionary, end it normally; nested
functional combinators will see that as the caller dictionary, as expected.

## Composition contracts

Before composing, put the two signatures adjacent:

```text
stem-step   ( record -- record )
blossom     ( record -- )
draw        ( record -- )
```

`/stem load fp-keep` turns a configured consuming stem into stem-step; following
it with a configured blossom gives draw, `{/stem load fp-keep blossom}`. This is
exactly the poster's pipeline. Make records dictionaries (`<< /X 1 /Y 2 >>`)
and read them with `get` or `fp-field`; positional arrays make every reader
remember which index meant what. It is easier to change one configured step than repeat palette,
counter and drawing setup throughout a layout loop. The imperative control
shows that the same graphics helpers are reusable in either style.

Use `/name load` when passing a named procedure; bare `name` invokes it.
Use `{}` for identity, `[]` for empty data. Callback inputs are actual values,
including nested arrays/procedures; they are not automatically destructured.
Use `aload pop` deliberately and leave exactly the required result count.

## Instrumentation and contracts

Wrap a mark with `<< /Calls 0 >> aop-count`, retain that dict explicitly, and
check its count against expected records. Wrap the whole score with
`(garden) aop-trace`, rather than printing hundreds of marks. Before hooks can
inspect copied inputs and after hooks copied outputs. Do not let the hook
consume the target's actual arguments or change graphics accidentally.

For failures, after is skipped and attempt counts remain. Around advice owns
proceed: `exec` calls the supplied target. There is no finally advice. Use
`--profile` for native timing and the external CPU benchmark for comparisons.
A deterministic counter is more portable than `usertime` (zero on wasm and
currently wall elapsed time on native).

## Debugging and common mistakes

| Symptom | Likely cause | Correction |
|---|---|---|
| Undefined procedure variable | name executed instead of passed | `/draw load`, not `draw`, at a higher-order call site. |
| `fp-literal-array-required` | quotation used as a collection | `[1 2 3]`, not `{1 2 3}`. |
| `fp-quotation-required` | literal array/operator/name used as callback | `{...}`, not `[...]`; wrap an operator in `{add}`. |
| `fp-callback-stack-effect` | extra/missing outputs, `dup` or unconsumed input | Re-evaluate callback with one value and a sentinel underneath it. |
| Predicate type guard | arithmetic result used as truth | Compare explicitly, e.g. `0 gt`. |
| Captured config changes later | composite aliases | Allocate/copy configuration deliberately; captures are shallow. |
| Names change behavior after `end` | assumed lexical closure | Curry the required data; names stay dynamically resolved. |
| Around does nothing | target never invoked | Keep/discard/proceed deliberately; `exec` is proceed. |
| After hook absent on error | assumed finally semantics | Handle the failure at an explicit caller-owned boundary. |
| No intercept of bound operator/old proc | binding/capture already fixed object | Decorate the actual call site; see AOP.md. |
| Clip/path/colour leaks | drawing or advice changes state | Own a balanced graphics wrapper in the mark. |

Use `--headless --pstack-on-error`, exact-name guard self-tests and small source
snippets. `--lint-strict` catches residual stack/save imbalance and blank
renders. An error may leave callback side effects; re-create input for a clean
retry. Never assume failed map returns a partial output. Same-depth caller
stack corruption is undetectable by the runtime guard: test actual values.
See [ENGINE-FRICTION](ENGINE-FRICTION.md) for diagnostic limits and
[IMPLEMENTATION](IMPLEMENTATION.md) before adding a combinator.


## Measured type and diagnostic advice

For a full text-record pipeline and a before/after/around proof that catches a
real invalid input, see [the live studies](WASM-STUDIO.md). Use built-in outline
fonts for portable charpath geometry. Font advances are not glyph ink bounds:
compute the latter deliberately, and remember that pathbbox is in current user
coordinates. AOP overlays belong to diagnostics; colour/style changes remain
explicit drawing configuration. The browser executes the same PS sources,
with no JavaScript substitutes for glyph geometry.
