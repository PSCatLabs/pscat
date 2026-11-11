## #1101: a faster executor for large kernels (2026-11-11)

fractal_clouds iterates at 1280×800, one frame per process, so the
executor's run is everything there. golden_coast at 1280×800 now runs in
13.1 s, down from 22–23 s. The bytes and the step counts are unchanged.

### What the time was

The issue guessed it was memory: a 4.4k-register file is 563 KB per worker,
and the IR is 128 bytes an instruction. Measuring said otherwise.

- **The lowered encoding alone gave nothing** (5.28 → 5.35 s on golden_coast
  640×400). This is a flat array of 16-byte instructions.
- **The register file does not matter.** At the epic head, mountains (96
  registers) and golden_coast (4.4k) cost the same per dispatch, about 40
  cycles. Their IPC was 4.4, which means compute, not stalls.
- **The cost was machine instructions per dispatch,** about 180 for a 16-lane
  op. A temporary in-crate microbenchmark timed each kind:
  - mul: 4 ns;
  - integer add: 16 ns;
  - CheckInt: 11 ns;
  - Gather: 32 ns. Gather is 5.5% of golden_coast's dispatches, so it was
    about a quarter of the time.

  Inside `Machine::block`'s one large function, LLVM splits by-value lane
  arrays (`[i64; 16]` from `.map`) into 16 scalars and spills them. The same
  loop in its own function vectorises.
- **A function per handler was worse.** Moving `bin` out of line made mul
  8 ns. `OpRef` went through memory, and a 10-register prologue ran on
  every call.

### What changed (each measured alone, golden_coast 320×200, one thread)

| step | kernel cycles |
|---|---:|
| epic head | 33G |
| branch-free fallible ops, operands read in place by shape | 29G |
| blend instead of a branch per lane for a partial store | 25G |
| `Gather1`, and `Gather` without per-lane branches | 24G |
| full-width writes at an interval's start (`Kernel::fresh`) | 22G |
| 8×2 batches | 20G |
| add/sub/mul as their own opcodes; TagAnd as Mul | 18G |

- **Fallible ops** (div, compares, sqrt, cvi, integer add/sub/mul, the checks)
  compute every lane and OR a fail flag across the batch. The mask is built
  on a cold path only when some lane failed. A failing lane holds whatever it
  computed, since it is retired and never stored. A select for it kept LLVM
  from vectorising the loop.
- **`Gather1`** handles a table read whose tags are constant and whose row or
  column is a good constant. Those are fractal_clouds' height and forest
  lookups (tables of up to 262k elements), and fixed-column reads of feature
  records. It checks one index, and every lane loads from a clamped index.
  The general form stays in the side table.
- **Full-width writes.** The allocator records, per body, the positions of
  the writes that start a register's live interval. On the lanes such a
  write skips, nothing in the register is live:
  - an interval sharing the register ended earlier in program order, and
    loops already widen intervals that cross them;
  - a lane that later reads this value is one that this write, or a later
    write on that lane, covers. That is the allocator's own invariant.

  So the store skips the blend. Marking every write this way fails at least
  nine of the kernel tests, the differential fuzz among them. The new test
  `writes_that_cover_every_lane_keep_each_lanes_values` covers merges,
  partly defined names, early exits and shared procedures.
- **8×2 tiles.** golden_coast at 1280×800 has 10.6 lanes active on average,
  against 10.0 for a 16×1 run. Each row still reports its last seeding pixel
  in raster order (`srand`, #1063). `final_rand_state` still scans rows.
- `PSCAT_KERNEL_TIMING=1` now prints a `kernel batches:` line, with the
  instructions run and the average lanes active.

### Results (M4, fractal_clouds ea18d58; the machine had other load)

Every render is `cmp`-identical to the epic head's, and the step counts
match.

| frame | before | after | |
|---|---:|---:|---:|
| golden_coast 1280×800 | 22.2–23.0 s | 13.1 s | 1.7× |
| golden_coast 640×400 | 5.28 s | 3.53 s | 1.5× |
| mountains 1280×800 | 12.7 s | 8.9 s | 1.4× |
| mountains 640×400 | 3.19 s | 1.96 s | 1.6× |

Golden_coast 1280×800 measured by kernel cycles is 594G → 370G (1.6×).
The cycle counts are steadier than wall time on a loaded machine.

Gallery kernel pieces, `kernel cpu run`:

| piece | before | after |
|---|---:|---:|
| mandelbulb | 3.77 s | 3.11 s |
| dreaming_network | 3.29 s | 2.50 s |
| mochi_in_oils | 2.03 s | 1.27 s |
| mirror_garden | 0.54 s | 0.37 s |

`scripts/bench_kernels.sh` showed no slowdown. The one-thread `kernel1`
column went 0.197 → 0.122 s on mandel, and 0.049 → 0.039 s on spheres.

### What did not pay

- **Sharing registers across subroutines** (candidate 2). The register file
  is not the cost (see above).
- **Coalescing moves.** Moves are 19% of dispatches, but the cheapest kind
  (2 ns). Under masked execution only the plain case is safe: the source's
  interval ends at the move, and the destination's starts there. Call
  parameter and result moves cross register regions, and the second branch's
  join copy interferes. That removes perhaps a quarter of them, about 3%.
- **4×4 tiles.** Between 16×1 and 8×2 on golden_coast. They tie 8×2 on
  mountains and the gallery.
- **Counting active lanes only when the mask changes.** No change.

### Open

- Moves coalesced in the allocator, for the plain case above.
- `Gather1` still compiles to scalar code inside its handler: about 17 ns,
  against 11 ns for the same loop on its own.
- Wider x86 SIMD and approximate math stay #1100.
