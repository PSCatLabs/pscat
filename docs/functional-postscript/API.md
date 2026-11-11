# API reference

Load in order:

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
[1 2 3] 10 {add} fp-curry fp-map == % [11 12 13]
```

Every public operation is a PostScript procedure, not a native operator.
Top of stack is at the right. `...` denotes an untouched caller prefix;
`q`, `p`, `target`, `advice` denote executable arrays (ordinary or packed).
`exec` invokes returned quotations; `/name load` retrieves a named quotation
without calling it. Calling a procedure by its executable name also invokes it.

## Common contracts

- Arrays are **literal** ordinary/packed arrays. Strings, dictionaries, and
  executable arrays used as sequences are rejected. Nested values are opaque.
- Quotations must be executable arrays; executable names/strings/operators are
  not accepted as callbacks. Wrap them in a procedure, e.g. `{add}`.
- Library constructors never execute or bind user quotations. Returned
  procedures and their captured composite values share references with inputs.
  Treat generated procedures as opaque; mutating their elements is unsupported.
- Callbacks must preserve the caller prefix, and balance dictionary and
  graphics save stacks. Collection callbacks see **only** the prefix and their
  published inputs; `currentdict` is the caller's current dictionary. A plain
  `def` has its ordinary effect there. Nesting and reentry are supported.
- Array traversals snapshot input **slots** at entry. They do not mutate input
  arrays. Later edits to original slots do not affect this traversal; mutations
  of nested shared composites remain visible. Map and filter return fresh
  literal arrays; the nested objects are not copied. A fold may return its
  initial object unchanged. `fp-each` is explicitly for side effects.
- Net operand depth is checked after each normally returning collection
  callback. This catches missing/extra outputs, not same-depth corruption of
  caller values. There is no static stack-effect inference or sandbox.
- Callback errors propagate unchanged; no library dictionary is open during
  callbacks. Failed calls do not roll back operand values, mutations, printing,
  drawing, or callback-owned dictionary/graphics changes. Ordinary `stop`
  propagates to the caller's `stopped`. An `exit` that targets the collection's
  own loop is rejected after the loop as `fp-callback-exit-not-supported`;
  use filter to select values. Loops *inside* callbacks can use `exit` normally.
- Names inside user quotations keep normal dynamic lookup. Explicit captures
  retain objects, not the dictionary environment. Helpers and native operators
  inside the library are captured at load time. Load with standard operators
  in force. No global standard operator is replaced.

## Functional procedures

### fp-compose

`( p q -- composed )`, with `composed: ( ... -- ... )` applying p then q.

Use to chain stack transformations. A fresh four-element executable array
holds references to both quotations, with explicit `exec` between them.
Neither quotation's elements are flattened. Composition is associative in
observable execution order when callbacks obey ordinary PostScript contracts;
array identity/introspection of the resulting wrappers can differ.

```postscript
(lib/functional.ps) run
3 {1 add} {2 mul} fp-compose exec == % 8
```

No input mutation, output validation, graphics wrapping, or effect inference.
Invalid p/q raises `fp-quotation-required`; invocation errors come from p/q.
Deeply nested constructed compositions can reach the interpreter's execution
stack limit; this is not an optimizing compiler.

### fp-curry

`( value q -- partial )`; `partial: ( ... -- ... value )` followed by q.

Explicit one-value partial application, useful for palettes, configuration,
and arithmetic parameters. The captured value can have any PostScript type.
A literal one-element box plus `aload pop` pushes even executable names,
strings, operators and quotations as data. Scalars are captured by value;
composites and q by reference. This is **not lexical name capture**.

```postscript
(lib/functional.ps) run
/offset 10 {add} fp-curry def
[1 2 3] /offset load fp-map == % [11 12 13]
```

Curry pushes the capture *above* existing arguments. For noncommutative
operations, `[1 2] 5 {sub} fp-curry fp-map` returns `[-4 -3]`.
No input mutation; invalid q raises `fp-quotation-required`; q owns its effects.

### fp-dip

`( ... value q -- ...' value )`, where q transforms the prefix to `...'`.

Runs q while hiding value on the execution stack, then restores the exact
object and executable attribute as data. Useful for keeping a record while
another quotation consumes its inputs.

```postscript
(lib/functional.ps) run
2 3 /saved {add} fp-dip % 5 /saved
pop == % 5
```

No mutation; invalid q raises `fp-quotation-required`. If q errors or stops,
value is not restored; there is no finally behavior.

### fp-map

`( ... array q -- ... results )`, `q: ( ... x -- ... y )`.

Left-to-right transform; exactly one result per input. Fresh literal output
array has the same length. Empty input returns a fresh empty array without
executing q. Nested inputs are passed as values, not traversed recursively.

```postscript
(lib/functional.ps) run
[[1 2] [] [3]] {{dup mul} fp-map} fp-map == % [[1 4] [] [9]]
```

No library mutation of input; shared results alias. Guards: array/quotation
contracts and `fp-callback-stack-effect`. Error on a later element leaves
previous callback side effects in place; no partial array is returned.

### fp-filter

`( ... array predicate -- ... selected )`, `predicate: ( ... x -- ... bool )`.

Stable selection of the original snapshotted values, not predicate outputs.
All-pass and empty results are fresh arrays. Predicates run once per value.

```postscript
(lib/functional.ps) run
[1 2 3 4] {2 mod 0 eq} fp-filter == % [2 4]
```

No library mutation of input; nested selected objects alias. Guards: common
contracts, depth, and `fp-predicate-must-return-boolean`. Integers are not
truth values. Predicate mutation of a nested object affects the selected object.

### fp-fold

`( ... array initial q -- ... result )`, `q: ( ... accumulator x -- ... next )`.

Left fold with explicit initial value; no separate empty-input behavior is
needed. Empty input returns the **same initial object**, without executing q.
Works with arbitrary accumulator types, including executable data.

```postscript
(lib/functional.ps) run
[1 2 3] 20 {sub} fp-fold == % 14 = ((20 - 1) - 2) - 3
```

The library only updates its private accumulator slot; callback mutation is
ordinary PostScript mutation. Guards: common contracts and callback depth.
There is no identity-free `reduce` alias in this version.

### fp-each

`( ... array q -- ... )`, `q: ( ... x -- ... )`.

Ordered effects, intended for rendering, output and explicit mutation.
Empty input does nothing; the sequence slots are still snapshotted.

```postscript
(lib/functional.ps) run
[1 2 3] {=} fp-each % prints one integer per line
```

No collection result. Guards: common contracts and callback depth. Graphics
changes from q remain unless q uses its own balanced `gsave`/`grestore`.

## Aspect procedures

Advice runs on the same operand/dictionary/graphics state as the target.
To preserve a target's contract, before/after advice has effect `( ... -- ... )`
and must preserve actual values, not just depth. There is deliberately no
runtime advice-depth check: a generic target can have an arbitrary stack effect.
Tracing and counting meet the zero-net-effect convention. For detailed
interception and error behavior see [AOP.md](AOP.md).

| Procedure | Inputs / output | Invocation order | Mutation and errors |
|---|---|---|---|
| `aop-before` | `target advice -- decorated` | advice, then target | No constructor mutation; quotation guards. Advice error prevents target. |
| `aop-after` | `target advice -- decorated` | target, then advice | No constructor mutation; quotation guards. Target error/stop/exit skips advice. |
| `aop-around` | `target advice -- decorated` | push target **as data**, call advice | No constructor mutation; quotation guards. Advice owns proceed via `exec`, including skipping/repeating it. |
| `aop-count` | `target counter -- decorated` | increment `/Calls`, then target | Mutates the supplied dict on invocation, including failed target entries. Constructor validates only. Counter guards run again at entry. |
| `aop-trace` | `target label -- decorated` | print `enter label`, target, print `leave label` | No state mutation beyond stdout; string label aliases. Leave line on normal return only. |

Counter must be a dictionary with `/Calls` a nonnegative integer. Other keys
are preserved. Guards are `aop-counter-dictionary-required`,
`aop-counter-Calls-required`, `aop-counter-Calls-nonnegative-integer-required`.
Use a writable dict on standard PostScript. Integer overflow eventually promotes
to real under PS arithmetic; a subsequent invocation then rejects that counter.
Trace label must be a string (literal or executable; it is always treated as
data), or `aop-trace-string-required` raises. Newline characters in labels are
printed literally; labels are not escaped or treated as structured log records.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/counter << /Calls 0 >> def
/double {2 mul} counter aop-count (double) aop-trace def
3 double == % enter double, leave double, 6
counter /Calls get == % 1
4 {2 mul} {exec 1 add} aop-around exec == % 9
```

Before/after/around are also profiling hooks: a zero-effect before or after
quotation may update an explicitly captured metrics dictionary. Prefer PSCat's
existing `--profile` for elapsed timing, inclusive/exclusive costs and call graphs;
see [TESTING.md](TESTING.md). The library implements no new timing clock.

## Error vocabulary

Named guards follow PSCat library convention: invoking an undefined descriptive
name raises `/undefined` with that name in `$error /command`. Do not define guard
names. A missing caller operand can raise standard `/stackunderflow` earlier.
Other standard errors (allocation/limits, callback arithmetic, permissions in
Ghostscript) retain interpreter behavior. `%%SelfTest` blocks in both libraries
pin exact guard names; they do not accept arbitrary failure as success.
