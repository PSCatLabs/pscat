# #845: native ridged multifractal and eroded fBm

Two new pscat-only operators in `src/ops/noise.rs`, with bit-equal
PostScript twins in `lib/noiseref.ps`:

| operator | stack |
|---|---|
| `ridgedmf2` | `x y oct gain lac offset wgain angle - v` |
| `erodedfbm2` | `x y oct gain lac angle - v` |

They are the two octave loops at the top of fractal_clouds' profile
(`fc-ridged`, `fc-eroded`; #736 comment, "Still slow" item 3), which
`ridged2`/`fbm2` could not express.

## Design calls

- **Names.** `ridgedmf2` and `erodedfbm2` (and `nrridgedmf2`,
  `nrerodedfbm2`) occur nowhere else in the repo (`git grep -w`). A new
  operator, not options on `ridged2`, so `ridged2` keeps its single
  signature and stays bit-identical.
- **Operand order.** `oct gain lac` first, as in `fbm2`/`ridged2`, then the
  new operands. The issue's sketch had `lacunarity gain`; consistency with
  the existing family won. `gain` keeps the family's meaning (the amplitude
  ratio per octave). Musgrave's `H` is `gain = lac^-H`, and his feedback
  `gain` is `wgain` here.
- **Ridged multifractal** follows Musgrave (*Texturing & Modeling*,
  `RidgedMultifractal`). `sig = (offset - |n|)^2 * w`, then the next
  weight is `w = clamp(sig * wgain, 0, 1)`, the *weighted* signal, as in
  his code. `fc-ridged` uses the unweighted one. His `weight > 0.001`
  early exit is dropped, because it makes the sum depend on a threshold.
  `fc-ridged` writes `1 - |1.6 n|`, clamped at 0, squared, with weight
  `2 * sig`. Since `offset - |1.6n| = 1.6 (offset/1.6 - |n|)`, `offset 0.625`
  with `wgain 5.12` (2 x 1.6^2) gives the same shape, scaled by 2.56,
  except for the clamp at 0 (the square folds negatives back up).
- **Eroded fBm** follows Quílez's widely used derivative-damped fBm
  (`d += n.yz; a += b * n.x / (1 + dot(d, d))`). The gradient is summed as
  sampled: no chain rule for the frequency, and no rotation into the base
  frame. `fc-eroded` co-rotates its accumulator, while the common
  formulation does not. At angle 0 the two coincide, and the look is the
  same. The term is `amp * (n / (1 + |d|^2))`, which shares the octave walk
  with `ridgedmf2`.
- **Real octave counts.** `full = floor(oct)` whole octaves, and a
  remainder `r` adds one more whose term is scaled by `r` (Musgrave's fBm
  remainder). The result is linear in `r` between whole counts, so detail
  fades in continuously with distance. `oct` may be an integer or a real in
  0..=32; anything else, NaN included, is a `rangecheck`.
- **Rotation.** `angle` is in degrees. `lc = lac * cos(angle)` and
  `ls = lac * sin(angle)` are computed once per call, and each octave
  steps `p' = (lc px - ls py, ls px + lc py)`. The `k * SHIFT` offset of
  `fbm2` is kept, so the octaves do not all vanish together at the
  origin, which rotation alone leaves fixed. `cos`/`sin` go through
  `fmath` exactly as the PostScript operators do (`fmath::cos(angle.to_radians())`),
  so the reference just says `angle cos`.
- **Division.** `erodedfbm2` divides once per octave. IEEE division is
  correctly rounded, so it is as deterministic as a multiply. The module
  header's "no division" rule now names this exception.
- **Not normalized**, like the rest of the family. To normalize, divide by
  the sum of the amplitudes, `1 + gain + ... + r * gain^full`.
- `erodedfbm2` uses a pure-f64 `g2d` (gnoise2d without the `Num` detour).
  It computes the same expressions, and the bit-equality test through
  `nrgnoise2d` covers it. `gnoise2d` itself is unchanged.

## Tests

`tests/noise.rs` checks that both operators equal their references bit
for bit. It runs 40 points x 8 octave walks x (1 + 4 ridged shapes) x 4
seeds, with octave counts 0, 0.3, 2.5, 4.999, 5, 5.0 and 32.0, angles 0,
37, -90, 360 and 21.5, a negative gain, an offset below gnoise2's reach,
and wgain negative and large, so the clamp reaches both edges.

The test is sensitive to rounding. Changing only the association of one
addition in the reference (`(1 + dx²) + dy²`) made 94 of 1600 samples
differ, and dropping the lower clamp made 102 differ. It also pins four
values and checks:
- the edge cases (oct 0, `5` against `5.0`, one octave against the base
  formula);
- the fractional fade-in (oct 3.5 is the midpoint of 3 and 4, and oct
  3.000001 is next to 3);
- the errors;
- that a `--sweep-seed` reaches both operators.

## Benchmark (pscat only, Apple M4, release build, machine shared, load ~9)

Microseconds per loop iteration, the best of 3 runs of 200,000
iterations. Each iteration also makes its coordinates (two multiplies
and a `dup`), so the empty loop is the floor.

| call | µs |
|---|---|
| empty loop iteration | 0.13 |
| `ridged2`, 5 octaves (for scale) | 0.23 |
| `ridgedmf2`, 5 octaves | 0.29 |
| `erodedfbm2`, 5 octaves | 0.23 |
| `nrridgedmf2` (reference, 5,000 iterations) | 25 |
| `nrerodedfbm2` (reference, 5,000 iterations) | 48 |

Both are well under the 1 µs target, against 6.0 and 8.4 µs for
fractal_clouds' PostScript loops around the native primitives.
