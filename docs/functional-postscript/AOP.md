# Aspect-oriented instrumentation

[aspect.ps](../../lib/aspect.ps) has five public constructors. Their join point
is one explicitly captured target quotation. There is no aspect registry,
pointcut expression language, automatic interception, or replacement of
standard operators. See [API](API.md) for exact signatures and guards.

## Advice and ordering

- `aop-before` composes advice then target.
- `aop-after` composes target then advice; it means **after normal return**.
- `aop-around` curries target as data into advice. Advice has one additional
  input, target, and may use `exec` as proceed, discard it, or invoke it twice.
- `aop-count` curries a caller-owned dictionary into `aop--tick`, then installs
  that hook as before advice. `/Calls` counts attempted entries, including
  target errors. Nested counters can share or use different dictionaries.
- `aop-trace` curries a string into two print hooks and uses before/after.
  Labels, including executable strings, are pushed as data; they alias.

For `target beforeA aop-before afterB aop-after`, order is beforeA, target,
afterB. Adding another before hook outside this wrapper runs that new hook
first. With two trace wrappers, output nests: outer enter, inner enter, target,
inner leave, outer leave. Calls repeat with the same captured objects; there is
no hidden global instrumentation state.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
3 {2 mul} {exec 1 add} aop-around exec == % 9
3 {2 mul} {dup fp-dip exec} aop-around exec == % 12, proceeds twice
3 {undefined_target} {pop 4 add} aop-around exec == % 7, skips target
```

## Stack, graphics and errors

Advice operates on the target's actual stack. A tracing or contract hook must
leave the same values and graphics state unless its documented purpose is to
change them. Before sees inputs, after sees outputs. The constructors cannot
infer the target's effect and do not check advice arity. Use map/each guards
when a decorated target is used as a collection callback.

Neither constructors nor built-in hooks issue `gsave`/`grestore`, change the
path, clip, CTM, colours or RNG. Counter hooks mutate only the explicit
`/Calls` entry. Trace hooks write normal stdout through `print`, so the same
lines reach PSCat stdout capture and the browser runtime. Trace printing can
be expensive and can fail under output limits; an enter failure prevents the
target, and a leave failure occurs after the target's effects.

No error is caught or replaced. Before failures prevent target entry; target
failures skip after; after failures propagate after successful target effects.
`stop` and `exit` follow ordinary PostScript unwinding. There is no finally
advice, transaction, implicit dictionary cleanup, graphics recovery, or rollback.
Around advice may implement its own `stopped` handling, but must preserve the
original error deliberately and must not mistake an ordinary `stop` for an
error. Unbalanced target-owned state stays the target's responsibility.

## Interception boundaries

Explicit capture freezes an **object reference**, not all name lookups in its
body. If you redefine `/draw` after capturing `/draw load`, the old capture
still points at the old procedure. If that old body calls an unbound `paint`
name, however, it will see paint's current binding. Mutation of the *same*
procedure array remains visible through existing captures.

Replacing a user dictionary binding explicitly can intercept dynamic calls:

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/paint {2 mul} def
/old /paint load def
/counter << /Calls 0 >> def
/paint /paint load counter aop-count def
3 paint == % 6, counted
3 old ==   % 6, not counted
counter /Calls get == % 1
```

`bind` replaces names resolving to native **operators**, not ordinary procedure
names. A procedure bound before `/add` is explicitly decorated will keep its
captured native add; unbound `{add}` will resolve the new binding. The regression
test `operator_binding_and_previous_captures_limit_interception` pins this
boundary without changing global operators in the shipped library. Do not use
operator replacement for artistic transformations; compose drawing steps.

## Profiling hooks

Before/after/around quotations are hooks for logging, contracts or explicit
metrics. A zero-effect before hook can increment a second counter or inspect a
copied input, and an after hook can validate a copied result. There is no new
profiling clock in aspect.ps. PSCat already provides:

```sh
target/ci/pscat --page 720x960 --headless --profile --profile-graph \
  examples/functional/poster.ps
```

Native reports use named procedure calls, inclusive/exclusive elapsed time,
and optional edges. A captured procedure invoked only by `exec` is anonymous
to named profiling; define a named wrapper or call a named artistic primitive
(as the demo does) to make its work visible. Native profiler times are elapsed
wall time, not CPU. Native `usertime` also currently uses a wall clock and
reads zero on wasm; see [engine friction](ENGINE-FRICTION.md). For paired CPU
measurements use the external benchmark script. A counter is deterministic;
timing output and diagnostic trace I/O are not a reproducibility hash of art.
