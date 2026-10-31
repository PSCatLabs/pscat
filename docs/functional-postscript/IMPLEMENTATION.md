# Implementation notes

Read the [architecture](ARCHITECTURE.md) for boundaries and [API](API.md) for
contracts. All source references below are to named procedures, so line shifts
do not make the documentation stale.

## Quotation construction

`fp-compose` validates both inputs using `fp--quote`, opens a temporary
two-entry dictionary, then constructs:

```text
[ p <captured exec operator> q <captured exec operator> ] cvx
```

The temporary dictionary closes before returning. p/q are executable array
elements and therefore push as data when encountered. Each following `exec`
invokes one. Concatenating the *elements* of p and q would be a different
representation; retaining the two original arrays preserves aliases and
quotation boundaries. Construction cost is independent of body lengths.

`fp-curry` builds:

```text
[ literal-box <aload> <pop> q <exec> ] cvx
literal-box = one-element literal array containing captured value
```

`aload` pushes the box contents without executing them. This matters for
executable names, strings and operators: putting them directly into a procedure
would run them. Captured executable arrays are also returned unchanged as data.
`fp-dip` composes q with `value {} fp-curry`, invokes that wrapper, and thereby
keeps value entirely off the operand stack until q returns.

Constructors may use scratch dictionaries because they do not call user code.
Never replace `/fp.q load` or `/aop.label load` with bare names: a procedure or
executable string reached through a name executes instead of becoming data.
Native operators in generated arrays are inserted with `/name load` while
library helpers themselves are bound. Standard operators must be in force
when the library loads; this is the repository's existing `bind` convention.

## Traversal implementation

All collection procedures validate inputs and call `fp--walk` with
`(array initial quotation kind)`. The private kind is `/map`, `/mapi`,
`/filter`, `/fold` or `/each`; it is not a public option. `fp--walk` looks the
kind up once in `fp--plans`, which gives a specialized prepare, collect and
finish helper plus the result count and whether an output array is needed.
The per-element path therefore never compares the kind; that dispatch was
about half of the traversal overhead. `fp--walk` allocates a fresh frame:

| Slot | Meaning |
|---|---|
| fp.q | caller quotation, never invoked by lookup |
| fp.acc | fold accumulator, arbitrary PS value |
| fp.seq | fresh shallow snapshot of source slots |
| fp.expect | caller operand depth after all API operands were consumed, plus the kind's result count |
| fp.i | current snapshot index |
| fp.used | filter's selected count |
| fp.out | fresh output array for map/filter, otherwise null |

`count ... /fp.expect exch def` deliberately measures before pushing the key.
The apparently simpler `/fp.expect count def` includes that key in the baseline.

The returned-and-immediately-executed program is structurally:

```text
[ length
  [ frame begin prepare exec q exec frame begin collect exec ] cvx repeat
  frame begin finish exec
] cvx
```

Every word shown above as an operator/helper is a captured object, not a
run-time helper-name lookup. The `fp--prep-*`, `fp--collect-*` and
`fp--finish-*` helpers end the frame they receive. Prepare pushes only callback
inputs (the element; the index too for `/mapi`; the accumulator first for fold),
then ends. Collect checks actual depth against fp.expect.
For filter it then checks the result type, consumes the boolean, and copies
the *snapshotted input value* if selected. For map it stores the output, for
fold it updates fp.acc, and for each it just advances the cursor.

Finish rejects incomplete iteration (callback `exit` skipped collection),
returns the correct result and closes the frame. Do not keep the loop in a
frame slot: the loop already references frame and a reciprocal reference would
create a never-freed cycle under `Rc`. The frame has no reference to either
generated program. Callback-created cycles remain an ordinary PS/engine issue.

## Allocation and complexity

Every traversal is O(n) slot operations plus callback cost. It allocates a
snapshot of n slots even for fold/each. Map/filter also allocate n output
slots. Filter returns a `getinterval` view of its fresh output; its backing
storage retains n slots even for a short selection (unused slots are null).
Nested composites are shared. There is no lazy sequence protocol, chunking,
parallel callback execution, deep copying or fusion of map/filter/fold.

One small loop quotation is reused for all elements; no per-element generated
program is allocated. Native repeat frames keep execution depth independent of
sequence length. Nested combinators and deeply nested compositions still use
normal execution stack space. Measurements are in [testing](TESTING.md).

The frame and snapshot buys stable visitation, reentry, no shared globals,
visible callback scope, and net-depth diagnostics. It costs more dispatch and
allocation than a purpose-built native/imperative loop. Do not optimize these
properties away without preserving the tests or deliberately revising the API.

## Extending safely

Add a primitive only for a real composition use case. Reuse constructors rather
than inlining arbitrary executable data. Capture helpers explicitly, bind the
library's own operators, and do not bind user code. A new traversal mode needs
all four paths (validation, prepare, collect, finish), plus empty/error/nested
coverage. Keep every named guard next to an exact-name `%%SelfTest`; keep both
libraries on `selftest/libraries.txt`. Every top-level definition needs either
public capability tags or `@internal`.

Exercise callback `def`, `currentdict`, `count`, stop, errors, alias mutation,
packed quotations under real gs, and repeated invocation. Use
`common::scratch_dir` for any new Rust test artifact. Pixel equality belongs
in Rust; guard tests belong in PostScript. No normal callback should see an
internal frame or an internal stack operand. See [testing](TESTING.md).
