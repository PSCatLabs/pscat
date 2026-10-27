# Functional PostScript design

Status: implementation plan, 2026-10-27. The implemented contracts will live
in API.md; progress and validation evidence live in PROGRESS.md.

## Scope and integration

Two ordinary libraries, `lib/functional.ps` and `lib/aspect.ps`, expose
prefixed names (`fp-`, `aop-`). They use existing Level 2 operators only.
No parser, engine, dependency, runtime, operator replacement, or automatic
weaving. The libraries participate in the existing capability tags, embedded
library loader, self-test manifest, and Rust integration tests.

The implementation uses small, explicit quotation constructors:

| Primitive | Stack effect | Callback |
|---|---|---|
| fp-compose | p q -- composed | p then q |
| fp-curry | value q -- partial | pushes captured value, then q |
| fp-dip | value q -- value | q runs on the stack below value |
| fp-map | array q -- array | x -- y |
| fp-filter | array q -- array | x -- boolean |
| fp-fold | array initial q -- result | accumulator x -- accumulator |
| fp-each | array q -- | x -- |
| aop-before | target advice -- decorated | advice then target |
| aop-after | target advice -- decorated | target then advice, normal return only |
| aop-around | target advice -- decorated | advice receives target quotation |
| aop-count | target counter -- decorated | increments counter /Calls before entry |
| aop-trace | target label -- decorated | enter/leave lines on stdout |

Array operations accept literal arrays, including nested values. Quotations
are executable ordinary or packed arrays. Callbacks preserve the caller's
stack prefix and balance their dictionary and graphics stacks. Check net
operand depth after each collection callback, plus filter's boolean type.
No static effect inference, recursive traversal, implicit deep copies,
finally advice, lexical name binding, or pointcut language in this version.

## The semantic problem to solve first

PSCat's Rust interpreter has an explicit execution-frame stack. An executable
array encountered as a procedure element is pushed; one resolved through a
name or `exec` runs. `load` retrieves without invoking. Composite values alias.
Names resolve dynamically through the dictionary stack; `bind` captures native
operators, not arbitrary procedure names.

A combinator must not execute a caller procedure under its scratch dictionary:
that changes `def` and `currentdict`. Collection calls allocate a fresh state
dictionary, construct a loop quotation containing that dictionary as a literal,
and `end` before invoking the loop. Each iteration briefly opens state to
prepare arguments, closes it for the callback, then opens it to collect the
result and closes it again. No private state is on the operand stack during
callbacks. Captured helpers use immediate lookup (`//`) and native operators
are bound. Nested calls allocate independent frames. A callback error propagates
without leaving a library dictionary open. Callbacks that themselves leave a
dictionary open are outside the contract; the library is not a sandbox.

Curry boxes captured data inside a literal one-element array and unboxes with
`aload pop`. Merely inserting an executable name or string into a generated
procedure would execute it. This boxing preserves arbitrary data attributes.
Constructors retain references to quotations and composite values. They never
call `bind` on user code or promise deep immutability.

## Research and decisions

[Joy](https://hypercubed.github.io/joy/html/j02maf.html) motivates quotation
composition, `dip`, and reasoning in terms of stack transformations. Iteration
uses native `repeat`, rather than a family of recursive combinators.
[Factor](https://github.com/factor/factor) motivates explicit stack contracts,
sequence operations, curry, and composition in execution order. Its
[stack checker](https://docs.factorcode.org/content/article-inference-combinators.html)
needs information this library does not have; inference is future tooling.
[Cat](https://github.com/cdiggins/cat-language) reinforces that a stack effect
can be a function type; this project adopts documentation, not its type system.
The [PLRM, third edition](https://www.adobe.com/jp/print/postscript/pdfs/PLRM.pdf),
sections 3.3, 3.4, 3.5, 3.7, 3.11, 3.12 and 4.2, is the authority for procedure
execution, aliasing, dictionaries, errors, binding and graphics state.
[AspectJ's advice model](https://eclipse.dev/aspectj/doc/latest/progguide/language.html)
inspires explicit before/after/around decoration. Our join point is one captured
procedure invocation; there are no global pointcuts. `after` means after normal
return, never finally. Around advice may skip or invoke its target more than once.
No third-party implementation is incorporated.

## Demonstration and validation plan

A deterministic psychedelic flower concert poster shares drawing primitives
between functional and imperative drivers. Build records with map, select with
filter, compute a checksum with fold, and draw with each. Capture a palette
as data to reuse geometry across three styles. Decorate the flower procedure
for counting; trace the whole composition. Use native profiling rather than
inventing a less accurate library clock. Compare complete pixel buffers between
drivers. Save actual PNGs, a detail and pipeline images for offline HTML.

Self-tests cover all guards. Rust integration tests cover nested invocation,
caller dictionary identity and definitions, arbitrary executable captured data,
aliasing, repeated execution, errors, native-operator binding, graphics state,
and stdout. A real Ghostscript check covers packed procedures and semantic
agreement. Full build, ci-profile tests, clippy, formatting, selftests, links,
licences, docs examples and browser screenshot validation finish the work.

## Workflow exception

The user's experiment replaces ordinary issue/PR/reviewer/merge orchestration:
one long-running `feature/functional-postscript` branch, checkpoint commits,
and a later independent Claude Opus review. No main merge or automated reviewer.
Core changes would be limited to small blocking bugs with evidence. Findings
are recorded in ENGINE-FRICTION.md; the normal engine-refactor escalation rule
is not permission to expand this effort.
