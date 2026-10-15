# #515: `currentrgbcolor` rounds its readback like gs (2026-10-14)

Follow-up to `notes/505-grestore-bottom-currentgray.md` (which left
`currentrgbcolor` alone) and `notes/513-readback-operators.md`.

gs prints `0.3 0.6 0.9 setrgbcolor currentrgbcolor` as `0.3 0.6 0.9`; pscat
printed the widened f32s `0.30000001192092896 0.6000000238418579
0.8999999761581421`. `currentrgbcolor` now pushes each component through
`f32_readback` (`src/ops/graphics.rs`), as `currentgray`,
`currenthsbcolor`, `currentcmykcolor` and `currentcolor` already do.

A read-then-set round trip still lands on the same `f32`, so
`currentrgbcolor setrgbcolor` changes nothing (tested in
`tests/grestore_boundary.rs`). What does change is arithmetic on the value:
`lib/paintkit.ps`, `lib/drykit.ps`, `lib/drychalk.ps` and
`lib/drycharcoal.ps` do lerps on `[ currentrgbcolor ]`, and the rounded
inputs can differ from the widened ones by a few f32 ulps.

Pinned in `tests/grestore_boundary.rs` (`[0.3 0.6 0.9]`, and
`[0.123 0.456 0.789]`, both checked against gs 10.07.1).

## Identity and regenerated renders

See the PR description for the exact figures.
