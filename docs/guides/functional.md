# Functional PostScript

`lib/functional.ps` adds `map`, `filter`, `fold` and friends to PostScript,
so a picture can be written as data in, marks out: build a list of records,
keep the ones you want, draw each one. `lib/aspect.ps` wraps a procedure
with code that runs before, after or around it, which is handy for counting
how many marks a piece drew or tracing where it spent its time.

Both are plain PostScript with no dependencies. They are compiled into
`pscat` (0.7.0 and later), so `(lib/functional.ps) run` works from the
command line, from `pscat-mcp` and in the browser build, with no checkout.
They also run in Ghostscript.

```postscript
(lib/functional.ps) run
[1 2 3] {dup mul} fp-map ==            % [1 4 9]
[1 2 3 4] {2 mod 0 eq} fp-filter ==    % [2 4]
[1 2 3] 0 {add} fp-fold ==             % 6
```

![Chromatic Bloom: thirteen flowers on long stems over a dark ground, drawn by a program that maps, filters and folds a list of flower records](../functional-postscript/assets/ultraviolet.png)

*Chromatic Bloom* (`examples/functional/poster.ps`) is 19 lines of driver:
25 candidate sites are mapped to flower records, filtered to the 13 that
fit, folded into a checksum, and drawn by one composed stem-then-blossom
procedure. `examples/functional/imperative.ps` draws the same pixels with
ordinary loops, for comparison.

## Loading

```postscript
(lib/functional.ps) run    % fp- procedures
(lib/aspect.ps) run        % aop- procedures; load functional.ps first
```

## Working with arrays

Collections are literal arrays, `[...]`. Callbacks are procedures, `{...}`.
Stack effects below put the top of the stack on the right.

| Call | Result | Callback |
|---|---|---|
| `array q fp-map` | a new array, one result per element | `( x -- y )` |
| `array q fp-filter` | a new array of the elements q accepted, in order | `( x -- bool )` |
| `array init q fp-fold` | the left fold, starting from `init` | `( acc x -- acc )` |
| `array q fp-each` | nothing; q runs once per element, for drawing or printing | `( x -- )` |

An empty array gives an empty result (or `init`, for a fold) without calling
q. `fp-fold` always takes an explicit starting value.

The callback runs in your own dictionary, with nothing of the library's on
either stack, so a `def` inside it lands where it would in a plain loop and
it can read values you left below its input. The library checks the stack
depth after every call and stops with a named error if the callback left
too many or too few values.

## Building procedures

| Call | Result |
|---|---|
| `p q fp-compose` | a procedure that runs p, then q |
| `value q fp-curry` | a procedure that pushes `value`, then runs q |
| `value q fp-dip` | runs q with `value` set aside, then puts `value` back |

`fp-curry` is how a callback gets its settings: capture a palette, a size or
a counter once, and pass the result to `fp-each`. The captured value is
pushed as data, even when it is a name or a procedure.

```postscript
(lib/functional.ps) run
[1 2 3] 10 {add} fp-curry fp-map ==         % [11 12 13]
3 {1 add} {2 mul} fp-compose exec ==        % 8: (3 + 1) * 2
```

These procedures return a new procedure; call it with `exec`, or store it
with `def` and call it by name. To pass a procedure you have already
defined, use `/name load`: a bare `name` runs it.

## A worked example

Twelve dots as `[x y r]` records, the large ones picked out by a filter, the
total area by a fold, and a curried `dot` for each colour. The counter from
`aop-count` (next section) confirms how many dots were drawn.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/dots [0 1 11 {} for] {        % i -- [x y r]
  dup 30 mul 25 add exch
  dup 37 mul 7 mod 15 mul 40 add exch
  3 mod 4 mul 6 add
  3 array astore
} fp-map def
/big dots {2 get 10 gt} fp-filter def
/area dots 0 {2 get dup mul 3.1416 mul add} fp-fold def
/dot {                         % [x y r] rgb --
  gsave aload pop setrgbcolor
  aload pop newpath 0 360 arc fill
  grestore
} def
/marks << /Calls 0 >> def
/teal [0.1 0.5 0.55] /dot load fp-curry marks aop-count def
dots /teal load fp-each
big [0.9 0.4 0.2] /dot load fp-curry fp-each
(dots drawn: ) print marks /Calls get =     % 12
(total area: ) print area cvi =             % 4172
showpage
```

Run it with `pscat --page 380x160 --png dots.png dots.ps`.

## Wrapping procedures

Each of these takes a procedure and returns a new one; nothing is redefined
behind your back. To change what a name does, `def` the result yourself.

| Call | The new procedure |
|---|---|
| `target advice aop-before` | runs advice, then target |
| `target advice aop-after` | runs target, then advice (only if target returned normally) |
| `target advice aop-around` | runs advice with target pushed as data; advice calls it with `exec`, or skips it |
| `target counter aop-count` | adds 1 to `counter`'s `/Calls` entry, then runs target |
| `target (label) aop-trace` | prints `enter label`, runs target, prints `leave label` |

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/calls << /Calls 0 >> def
/double {2 mul} calls aop-count (double) aop-trace def
3 double ==                          % enter double, leave double, 6
calls /Calls get ==                  % 1
4 {2 mul} {exec 1 add} aop-around exec ==   % 9
```

Advice shares the target's stack and graphics state, so before and after
advice should leave both as it found them. There is no "finally": if the
target fails, after-advice does not run, though the counter has already
counted the attempt. For timing, use `pscat --profile`.

## Rules worth knowing

- **Captures are not closures.** `fp-curry` captures one value. Names
  inside your procedures are still looked up when they run, as everywhere in
  PostScript.
- **Arrays are copied one level deep.** A traversal works on a copy of the
  input's slots, so editing the input array mid-loop changes nothing; nested
  arrays and dictionaries are shared, as usual.
- **No `exit` out of a traversal.** It stops with
  `fp-callback-exit-not-supported`; select with `fp-filter` first. `stop`
  passes through to your `stopped`, and a loop inside a callback can `exit`
  normally.
- **Keep graphics balanced.** A drawing callback should do its own
  `gsave` ... `grestore`, and `newpath` if it must not paint a path left over
  from before.
- **Plain loops are fine.** For a fixed handful of petals or a hot inner
  loop, `for` and `repeat` are shorter and faster: each library call checks
  its inputs and copies the array.

## Errors

Bad input stops with `undefined` and a name that says what went wrong, the
same convention as the other pscat libraries:

| Name | Usual cause |
|---|---|
| `fp-literal-array-required` | a procedure `{1 2 3}` passed where an array `[1 2 3]` belongs |
| `fp-quotation-required` | an array, name or operator passed where a procedure belongs; wrap an operator as `{add}` |
| `fp-callback-stack-effect` | the callback left too many or too few values |
| `fp-predicate-must-return-boolean` | a filter callback returned a number; compare it, e.g. `0 gt` |
| `aop-counter-...` | the counter is not a dictionary with a non-negative integer `/Calls` |

`pscat --pstack-on-error` shows the stack at the failure.

## Further reading

The library was built as an experiment, and its design notes are kept in
[`docs/functional-postscript/`](../functional-postscript/README.md): the full
API contracts, the implementation, the concert-poster and typography
studies, test evidence and the engine friction found along the way.
