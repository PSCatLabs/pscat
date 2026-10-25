# #734: native noise operators

New pscat-only operators in `src/ops/noise.rs`:

| operator | stack | what |
|---|---|---|
| `noiseseed` | `seed -` | build the noise permutation table from an int (default: seed 0) |
| `gnoise2` | `x y - n` | 2D gradient noise, about [-0.71, 0.71], 0 on the integer lattice |
| `gnoise3` | `x y z - n` | 3D gradient noise (Perlin improved), about [-1, 1] |
| `gnoise2d` | `x y - n dx dy` | `gnoise2` plus its analytic gradient |
| `vnoise2`, `vnoise3` | `x y (z) - v` | value noise, in [-0.996, 0.996] |
| `fbm2`, `fbm3` | `x y (z) oct gain lac - v` | fBm of gradient noise |
| `vfbm2`, `vfbm3` | same | fBm of value noise |
| `ridged2`, `ridged3` | same | ridged sum, `(1 - abs n)` squared per octave |
| `.permnoise2` | `x y perm - n` | artkit's `noise2` formula off a caller's table |

## Design calls

- **Names.** `noise2`/`noiseinit` belong to artkit, so the issue's
  `noise2`/`noise3` became `gnoise2`/`gnoise3`. No name above occurs
  anywhere else in lib/, gallery/, examples/ or tests/ (checked with `git grep -w`).
- **Algorithm.** It is frozen and written down twice: in the module
  header and in `lib/noiseref.ps`, which reproduces every operator bit for
  bit in PostScript. It uses f64 add/sub/mul in a fixed order, plus
  `floor` and table lookups: no division, no libm, no FMA. `tests/noise.rs`
  compares the two over more than 2,000 samples per seed for four seeds,
  covering negatives, near-lattice points, values past the 256-unit wrap
  and 1e6, and pins exact reprs.
- **The 2D gradient noise is artkit's formula** (same hash, gradients,
  fade and operation order). That made artkit's switch possible.
- **Seed.** `noiseseed` shuffles with its own LCG (the constants `rand` uses,
  high bits for the index), so `rand` is never read or moved. Both
  directions are tested. `--sweep-seed` does not reach it. That is a
  follow-up if wanted.
- **fBm** is not normalized. Octave `k` is shifted by `k·(17.3203125,
  11.4140625, 7.6015625)`, exact binary fractions, so octaves don't all
  vanish on the same lattice points. `oct` is 0..=32 (`rangecheck`
  otherwise), so a hosted call stays bounded.
- **artkit `noise2` is now native**, via
  `systemdict /.permnoise2 known { /noise2 { Perm .permnoise2 } bind def } if`.
  `.permnoise2` reads `Perm` on every call (dict-stack lookup, as
  before) and computes with PostScript's own int/real promotion. So an
  all-integer lattice point still returns the same `0` (integer) or
  `0.0` as the procedure did. `tests/noise.rs` compares it with the old
  procedure (extracted from the file) by value *and* type. The
  two-binary `identity_check.sh` over every artkit/fillkit render is
  byte-identical. Two things change, neither of them visible: the
  n2*/nft*/g2* scratch names are no longer left defined, and
  redefining `gethash`/`grad2`/`nfade`/`nlerp` no longer changes
  `noise2`. Nothing in the repo reads or redefines either.
  Ghostscript keeps the procedure.
- `.permnoise2` is dot-named and undocumented outside artkit and
  HANDOFF, like `.sameclip`. The public operators have a single,
  untyped-overload-free signature.

## Benchmark (pscat only, Apple M4, release build)

µs per loop iteration over 200,000 iterations, min of 5 runs. Each
iteration also does three multiplies and two `dup`s to make coordinates,
so the "empty" row is the floor.

| call | interpreted PS | native |
|---|---|---|
| empty loop iteration | | 0.18 |
| artkit `noise2` (old procedure → `.permnoise2`) | 4.86 | 0.26 |
| 2D gradient (`nrgnoise2` → `gnoise2`) | 4.19 | 0.20 |
| 3D gradient (`nrgnoise3` → `gnoise3`) | 9.8 | 0.18 |
| 3D value (`nrvnoise3` → `vnoise3`) | 6.9 | 0.18 |
| 3D fBm, 4 octaves (`nrfbm3` → `fbm3`) | 47 | 0.24 |
| 3D value fBm, 6 octaves (`nrvfbm3` → `vfbm3`) | 50 | 0.24 |

A native call costs about as much as an empty loop step, as the issue
hoped. The reference fBm is slow because it is written for clarity (an
array of coordinates, a generic octave loop), not speed.
