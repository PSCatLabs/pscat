---
name: functional-postscript
description: Write pscat PostScript in a data-in, marks-out style with lib/functional.ps (fp-map, fp-filter, fp-fold, fp-each, fp-compose, fp-curry, fp-dip) and wrap procedures with lib/aspect.ps (aop-before, aop-after, aop-around, aop-count, aop-trace). Use when a piece is built from a list of records, or when you want to count or trace calls to a drawing procedure. Plain rendering uses the pscat skill; art libraries use psart.
---
<!-- Contributors: tests/skill_blocks.rs runs every postscript code fence under plugin/skills on a bare install and fails on any error. A fragment that cannot run alone is fenced as "postscript partial". -->

# Functional PostScript

Both libraries are compiled into `pscat` 0.7.0 and later; load them with
`(lib/functional.ps) run` and then, if you need wrappers,
`(lib/aspect.ps) run`. The guide is `docs/guides/functional.md`.

## The calls

Collections are literal arrays `[...]`; callbacks are procedures `{...}`.

| Call | Gives | Callback |
|---|---|---|
| `array q fp-map` | new array | `( x -- y )` |
| `array q fp-filter` | new array of accepted elements | `( x -- bool )` |
| `array init q fp-fold` | left fold | `( acc x -- acc )` |
| `array q fp-each` | nothing; for drawing | `( x -- )` |
| `p q fp-compose` | procedure: p then q | |
| `value q fp-curry` | procedure: push value, run q | |
| `value q fp-dip` | runs q under value, restores value | |
| `target advice aop-before` / `aop-after` | procedure with advice before / after | |
| `target advice aop-around` | advice gets target as data, `exec` runs it | |
| `target counter aop-count` | bumps `counter /Calls`, runs target | |
| `target (label) aop-trace` | prints `enter label` / `leave label` | |

## The pattern

Make records, select, then draw each with a curried mark. Count the marks to
check the piece drew what you meant.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
/dot {                                % [x y r] rgb --
  gsave aload pop setrgbcolor aload pop newpath 0 360 arc fill grestore
} def
/recs [0 1 5 {} for] {dup 15 mul 12 add exch 3 mod 2 mul 4 add 50 exch 3 array astore} fp-map def
/n << /Calls 0 >> def
/mark [0.8 0.3 0.2] /dot load fp-curry n aop-count def
recs {2 get 5 gt} fp-filter /mark load fp-each
n /Calls get ==                       % 4
showpage
```

## Mistakes to avoid

- Pass a named procedure as `/name load`; a bare `name` runs it.
- `{1 2 3}` is not a collection (`fp-literal-array-required`); an array or
  an operator is not a callback (`fp-quotation-required`): write `{add}`,
  not `/add load`.
- The callback must leave exactly its result: `fp-callback-stack-effect`
  means one too many or too few. Filters must return a boolean (`0 gt`, not
  a number).
- Do not `exit` out of `fp-each`; filter first. `stop` passes through.
- Curry captures a value, not a scope; names still resolve when run.
- Drawing callbacks own their `gsave` ... `grestore`.
- `aop-after` runs only on normal return; there is no finally.
- For a few fixed petals or a hot inner loop, a plain `for` is clearer and
  faster.

Errors are `undefined` with the guard name; `--pstack-on-error` shows the
stack.
