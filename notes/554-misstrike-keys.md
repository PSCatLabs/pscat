# #554: /MisStrike and /MisStrikeScale

The mis-strike in the strike layer of `lib/typewriter.ps` was a hard-coded 1% of
strikes at 4x the x offset and 2x the y (#467). Only `/Wear` and `/StrikeWear`
reached it, and they scale all variation together. It is now two keys, named for
the physical traits: the machine (loose or worn typebars, a sloppy segment) and
the typist (speed, how hard and unevenly the keys are hit).

## Added

- `/MisStrike [0..1]`, default 0.01: the fraction of strikes that are mis-struck.
  `tIsUnit` check, error `tw-misstrike-must-be-0-to-1`. The issue sketched a
  non-negative number; a fraction above 1 means nothing, so it is 0..1 like
  `/Ribbon` and `/WornBand`.
- `/MisStrikeScale [ratio]`, default 1: a plain multiple of the 4x/2x on both axes
  (one key, not separate x and y: the 4:2 ratio is the measurement).
  `tIsNonNeg`, error `tw-misstrikescale-must-be-a-non-negative-number`. 2 is 8x and
  4x, so only its y matches the pre-#467 look, and x goes past what the board showed.
  0.25 makes x a normal strike's and 0.5 makes y one; 0 puts a mis-strike dead on
  its cell.
- In `tw-setup`, `tw-set` and `tw-type` (`@param` lines, `[0..1]` and `[ratio]`),
  `%%SelfTest` guards for both errors, a pixel test
  (`mis_strike_keys_default_to_the_measured_board`), and the demo
  `docs/dev/design/typewriter/misstrike_554.ps/.png`.

## Why the defaults are byte-identical

The line is `4 uni tMisRate lt { ... sX 4 mul tMisScale mul ... }`. The `4 uni`
draw is taken for every strike whatever the rate, so no other draw moves, and
`x * 4 * 1` is exactly `x * 4`. A side effect worth knowing: the letters
mis-struck at one rate are a subset of those at a higher rate. Checked with
`scripts/identity_check.sh --loader lib/typewriter.ps` and a control that sets
the new keys (the base tree rejects them, so the control catches that and falls
back to the defaults).

## Presets: not set

A worn machine plausibly has looser typebars, and an electric one fewer
mis-strikes. The board does not measure either: strikes more than 1pt off
horizontally are 0.8% (Duncan, manual), 0.9% (Szilard, a manual with a worn ribbon) and 0.2%
(O'Brien, the one inferred-electric document), and `/electric`'s `/Wear 0.7`
already shrinks its jumps. Setting them in a preset would also change every
existing `/worn` and `/electric` page. So `/Preset` neither sets nor resets them,
and a sloppy-typist setting persists across a preset change. No separate
"typist" group: two keys, as the issue asked unless the evidence gave a reason.
