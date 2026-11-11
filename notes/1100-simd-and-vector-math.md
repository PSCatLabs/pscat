## #1100: x86 instruction-set tiers and vectorised `sin`/`cos`/`atan` (2026-11-11)

The issue asked for two things: wider SIMD on x86_64, and vector
transcendentals as a documented approximate mode, then a benchmark on AVX2
and AVX-512 machines. Both are built. The vector math came out exact, not
approximate, and the x86 benchmark could not be run from here.

### Where the time is (M4, `sample`, one thread)

| piece | share in `sin`/`cos`/`atan`/`atan2` (libm) | `pow`/`log` |
|---|---:|---:|
| Mandelbulb | about 63% | small |
| Dreaming Network | about 1% | about 3% |
| golden_coast (fractal_clouds) | under 1% | about 2.3% |

So vector math matters for ray-marched pieces, not for fractal_clouds.

### Vector math: `.kernelmath /fast` (`kernel/vmath.rs`)

- **What it is.** libm's own `sin`, `cos` and `atan2`, ported:
  - `k_sin`, `k_cos`;
  - `rem_pio2`'s medium case (three rounds) and its small multiples of pi/2;
  - `atan`'s four intervals.

  Every path is computed on every lane and the lane's own one selected
  (`sel`). The code uses 64-bit integers only and `&` rather than `&&`, so
  LLVM vectorises the loops.
- **Exactness.** The results are the exact path's bits. The tests compare
  every lane the vector code did against `fmath`, over 4M random inputs per
  function plus each range boundary, and pass on arm64 and as x86_64 under
  Rosetta. So this is not the approximate mode the issue allowed, and no
  error bound needs documenting. It is still opt-in, until it has had more
  use and review.
- **Cold lanes.** Lanes libm special-cases go to the scalar code, which also
  decides `0 0 atan`'s error:
  - NaN and infinities;
  - zeros;
  - a 1.0 denominator;
  - |x| past the medium case, about 9.4e7 degrees.
- **Four things that mattered for speed:**
  - **Out of line.** In `Machine::block` the lane arrays are scalarised, as
    #1101 found, so the vector entry points are separate functions.
  - **No integer `match`.** A `match` on an i32 quadrant and the runtime
    `cos` flag kept the first version scalar, at 8.9 ns a lane. With
    selects, a const generic and i64 throughout, the loops have no scalar
    floating-point instructions.
  - **Rare paths on demand.** The reduction's later rounds, and the
    no-reduction path below pi/4, are computed only when some lane of the
    batch needs them.
  - **Not inside `block` at all.** Inline handlers made `block` 6% larger
    (mochi_in_oils: 2.9% more instructions in exact mode). Even as calls,
    three new opcodes in the block loop's dispatch cost about 6% of the
    kernel's cycles on fractal_clouds' scenes, though those kernels never
    run them. fractal_clouds measured the slowdown on the test release, and
    the cycle counts confirmed it: mountains at 640×400 took 107–109G cycles
    against build2's 105–107G. So the fast forms now lower to the existing
    `L::Ext` side table, and `Machine::ext` vectorises them. The block
    loop's opcodes are #1102's again, at 103–105G, and golden_coast is at
    163G against build2's 164–165G.
- **Speed, per lane on an M4** (NEON, 2 f64s a vector):

  | | vector | scalar |
  |---|---:|---:|
  | `sin` | 5.9 ns | 9.5 ns |
  | `atan` | 6.1 ns | 13.7 ns |

  Mandelbulb, one thread, measured in cycles: 85–96G exact against 71–72G
  fast with the machine loaded; on a quieter one, 68.6G against 54.9G (21%
  fewer), through the side table. Dreaming Network and Mirror Garden were
  unchanged. Every picture tested was byte-identical.
- **Per run.** The mode is chosen in `Code::new` for each run, never stored
  in a kernel. `kernelarray` always runs exact.
- **Not done:**
  - `exp` (`pow`), `ln` and `log`. They are about 2–3% of the pieces
    measured, and musl's versions are table-driven.
  - Noise: an exact vector noise would likely be the bigger lever for
    fractal_clouds.

### x86_64 tiers (`exec::Isa`, `isa_dispatch!`)

- **What is duplicated.** The block loop, `Gather1`, `Ext` and the vector
  math are each compiled for three tiers:
  - the baseline, SSE2;
  - x86-64-v2, `sse3,ssse3,sse4.1,sse4.2,popcnt`;
  - x86-64-v3, v2 plus `avx,avx2,bmi1,bmi2,lzcnt`.

  `Isa::chosen` detects the CPU once. `PSCAT_KERNEL_ISA` forces a lower
  tier, and `PSCAT_KERNEL_TIMING` names the one in use. Other CPUs have one
  tier, with the same code as before.
- **No FMA.** No tier enables FMA. The emitted x86 assembly has no
  `vfmadd`/`vfnmadd` in any kernel function. The v3 copies use `ymm`
  registers throughout (`block_v3`: 4,164 `ymm` references), and v2 and v3
  use `roundpd` for `floor` and `round`.
- **Run under Rosetta** (an x86_64-apple-darwin build; Rosetta has SSE4.2
  but no AVX, so v3 falls back to v2):
  - `tests/kernels.rs` (111 tests) and `tests/kernel_shim.rs` pass;
  - the vmath exactness tests pass;
  - Mirror Garden, Mandelbulb (exact and fast) and fractal_clouds' mountains
    and golden_coast at 160×100 give the arm64 bytes under `base` and `v2`.
- **v3 is not run here.** It runs in CI's x86 runners, which have AVX2:
  `bytes_do_not_depend_on_the_instruction_set` renders under each tier and
  against `/interpret`.
- **AVX-512 is not a tier.** Its target features were stabilised in Rust
  1.89, and the crate's `rust-version` is 1.88. Raising that is the
  owner's call (`Cargo.toml`).

### arm64 is unchanged

- **Bytes and steps.** All 9 fractal_clouds scenes and the gallery have
  build2's bytes and step counts.
- **Instructions retired.** Mountains at 320×200 (one thread): 308G before,
  309G after. Mochi in Oils: 149.5G before, 151.6G after (+1.35%, the
  largest).

### Not measured: AVX2 and AVX-512 speed

There is no x86 machine here. To measure on an AVX2 or Zen 4 box, from the
repo root, with a release build:

```sh
for isa in base v2 v3; do
  PSCAT_KERNEL_ISA=$isa PSCAT_KERNEL_TIMING=1 ./target/release/pscat \
    --page 600x600 --png /tmp/m.png gallery/mandelbulb.ps 2>&1 | grep -E 'cpu run|batches'
done
PSCAT=./target/release/pscat scripts/bench_kernels.sh 5 kernel kernel1
```

Add `PSCAT_KERNEL_THREADS=1` (then 2, 4 and 8) for the Cloud Run sizes the
issue names. Put `/fast .kernelmath` at the top of a copy of a piece to time
the vector math too.
