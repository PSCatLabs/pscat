# Architecture

The shipped implementation comprises [functional.ps](../../lib/functional.ps)
and [aspect.ps](../../lib/aspect.ps). Everything callable is ordinary
PostScript. The only new Rust is integration tests in
[functional.rs](../../tests/functional.rs). No engine, manifest dependency,
semantic, release, or MCP changes are required.

## Integration boundary

```text
Artist / ArtColony agent
        | ordinary PostScript, (lib/...) run
        v
functional.ps <------- aspect.ps (explicit constructors)
        |                  |
        +--- procedures ---+
                 |
      PSCat's existing execution stack
                 |
      artkit / PaintKit / graphics operators
                 |
         tiny-skia canvas -> PNG
```

`run` follows existing resource resolution (`src/paths.rs`). The build script
already embeds `lib/**/*.ps`; it also derives capability rows from the `% @`
tags on every new public and private definition. No hard-coded loader table or
capability entries are added. `aspect.ps` declares functional.ps as its
`@requires` chain; metadata does not execute that chain on an ordinary `run`.
The caller loads both files. Load them in the dictionary that should own their
prefixed definitions; no `begin` is left open by loading either library.

Public names are prefixed `fp-` and `aop-`, with double-hyphen helpers internal
by convention. There are no unprefixed `map`, `filter`, `compose`, replacements
for `exec`, or patched system dictionaries. Existing programs that do not load
these libraries retain their original execution paths. The existing suite,
operator-identity test and gallery checks provide compatibility evidence,
not a claim of complete PostScript conformance.

## Execution and names

The actual core is Rust (`src/interp.rs`, `src/object.rs`). PSCat drives an
explicit stack of scanner, procedure, loop, stopped and continuation frames.
An executable array encountered directly in a procedure pushes as data;
`exec` or name resolution invokes it. This distinction lets constructors put
user procedures inside a new executable array without invoking them early.

`load` retrieves a procedure as data. Immediate lookup (`//fp--prepare`) embeds
an existing helper in a new library definition. Native `bind` resolves native
operator names at library-load time. User quotations supplied later are never
bound or rewritten by this library. Arbitrary remaining names inside them
resolve through the dictionary stack at invocation time.

Captured values are ordinary PostScript references. Keeping a value after its
original dictionary has been popped is safe when that value itself is
retained; it does not retain name bindings to that dictionary. `fp-curry`
therefore provides explicit data capture, not lexical environments.

## Traversal data flow

```text
literal input array -> shallow slot snapshot -> per-element callback
                               |                      |
                       fresh state frame         caller dictionary
                               |                      |
                       fresh output/accumulator <- net-depth check
```

Each traversal allocates a new dictionary and snapshots the source slots.
Its loop quotation contains references to that frame, its callback, and the
already-captured helpers/operators. The frame is closed before entering the
loop. Per iteration:

1. Open frame, push accumulator if folding, push element, close frame.
2. Invoke user callback in exactly the caller's dictionary environment.
3. Open frame, check net depth/type, collect the result, increment cursor,
   close frame.

The loop's closing quotation retrieves output and closes the frame. No private
operand remains during step 2. Nested invocations have independent frames.
The state dictionary never stores its generated loop, avoiding a reference
cycle in PSCat's reference-counted composite model.

## Effects and failures

Construction validates literal arrays and executable quotations. Collection
invocation validates callback net depth; filter additionally requires boolean.
These guards use the repository's named undefined-error convention. Ordinary
callback errors leave no library dictionary open and unwind through native
`stopped` behavior. No extra `stopped` is inserted, so callback error identity
and ordinary outward stop remain intact. No rollback of drawing, mutation or
operand state is promised. Callback-owned unbalanced dictionaries/graphics are
outside the contract; library guards are not a hostile-code boundary.

Graphics state belongs to the caller and drawing quotations. Observation
advice prints/counts without changing graphics; it does not impose a `gsave`.
Artistic procedures can use balanced `gsave newpath ... grestore` when they
need isolation. PaintKit's own path/state conventions continue to apply.

## Dependency and scope decisions

The functional library depends only on standard PostScript operators. The
aspect library depends only on functional.ps. The poster additionally loads
artkit and PaintKit. The Python reproduction and verification scripts use the
standard library. The introduction uses saved local images and vanilla JS. The additional live
studio instantiates the **same existing PSCat interpreter** compiled to WASM,
with its unchanged JS API and local compressed bytes; it does not reimplement
the drawing in JavaScript. See [WASM-STUDIO](WASM-STUDIO.md) for file-URL loading,
source inlining, size, licence provenance and native/WASM parity tests.
No external code has been copied from the inspiration projects.

See [decisions](DESIGN-DECISIONS.md) for rejected lexical dictionaries, implicit
weaving and a new type system; see [implementation](IMPLEMENTATION.md) before
altering the generated procedure layouts.
