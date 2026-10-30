# Design decisions

Decisions describe the implementation on this feature branch, pending Opus
review. [DESIGN.md](DESIGN.md) records the initial plan and primary sources.

## 1. Libraries, with prefixes

All functionality is standard PostScript, with `fp-`/`aop-` public names and
explicit loading. This tests the central hypothesis without new native
operators, a parser, a runtime or external dependencies. A vocabulary dictionary
would require `begin` around use or longer lookup syntax; prefixes fit PSCat's
existing catalog and flat library conventions. Unprefixed aliases were rejected
to preserve compatibility. We do not claim a module system.

## 2. Array-only, explicit identity

A literal array is a useful record collection for procedural art. Generalizing
to strings/dicts would introduce output-type, order and byte semantics with
little demonstrated value. `fp-fold` uses an explicit initial accumulator,
so empty collections have one consistent rule. No identity-free reduce alias,
lazy sequence abstraction, recursive tree walk or large combinator family.
`exec`, repeat and ordinary PS loops remain good tools.

## 3. Captured data, dynamic names

Curry boxes any value and pushes it on invocation. This handles executable data
safely and makes palette/configuration ownership explicit. Implicit lexical
dictionaries were rejected: `begin` would alter `def`/`currentdict` and
name lookup, and dictionary lifetimes alone do not make lexical closures.
Deep copies would also silently change normal PS alias semantics. No type
inference is implemented: Factor/Cat suggest a future checker, not permission
to mislabel runtime depth guards as a type system.

## 4. Execution captures, no callback scratch scope

Each collection invocation snapshots slots and captures private state in a
small generated loop. The frame is closed during callbacks. Shared global
scratch was rejected because nesting overwrites state; a private dictionary
left open across callbacks was rejected because caller definitions land there.
Hiding state *below* callback arguments on the operand stack was rejected
because `count`, stack operators and diagnostics would see it.

A frame containing its own generated body was rejected because the reciprocal
references create an Rc cycle. The chosen generated code points at state;
state points only at values and user code, never back at the generated loop.
Preconstructing one executable instruction group per element was rejected for
O(n) generated code; the repeat body is constant-sized. Full effect isolation
and rollback are neither affordable nor faithful to arbitrary PostScript.

## 5. Snapshot slots, share nested objects

Callbacks may edit their source array; this call still visits the original
slots in order. This predictable behavior requires a shallow snapshot for all
traversals, including each/fold. It costs an extra array allocation. Filter's
result retains output capacity for n slots. Direct `forall` is cheaper if an
artist does not need stable snapshots, nesting safety or depth diagnostics.
A microbenchmark quantifies the cost; no speed advantage is claimed.

## 6. Explicit advice, normal-return after

AOP provides constructors, not implicit weaving. Target references are captured
and installation into any named binding is explicit. Around receives a normal
quotation; `exec` is proceed. Before/after share the target stack and must obey
its contract. “After” means normal return; finally would require stopped/error
rethrow machinery and a policy for arbitrary operand/dictionary/graphics leaks.
That complexity was deferred. There is no global aspect registry or pointcuts.

The existing native profiler is more useful than a new library timer with
hidden operands, coarse clock resolution and unclear wasm behavior. Counting
and tracing remain simple decorators; advice constructors serve as generic
profiling/contract hooks. CPU benchmarks are external tooling.

## 7. Shared rendering control

The imperative and functional poster drivers share the same graphics helpers.
This isolates orchestration differences and makes pixel identity meaningful.
Neither approach owns better drawing primitives. Three colourways change data
rather than code; native graphics and PaintKit ink all of them. Fixed-size petal
loops stay imperative, since replacing them would add little clarity.
This is evidence for composable orchestration, not a controlled study of agent
reliability or a claim that functional code makes art inherently better.

## 8. Separate human and agent entry points

The HTML is an offline illustrated explanation with pre-rendered interactivity.
It does not pretend to execute PS in the browser. Markdown is the precise
maintenance reference; the concise plugin skill routes there. New documentation
lives under the user-requested `docs/functional-postscript/`, linked from normal
agent/user indexes. No hosting, release, npm or MCP workflow is modified.

## Future directions, deliberately unimplemented

Prioritize a real CPU clock/clear clock naming, source-aware error diagnostics,
and optional stack-effect tooling. Consider fused traversals or lightweight
frames only with measurements and equivalent semantic tests. Lazy streams,
lexical modules, recursive combinators, static types, deep immutability,
automatic weaving, finally advice and implicit operator interception are outside
this version. No enhancement is a prerequisite for using the implemented API.
