## Compiled per-pixel kernels: `kernelimage`, CPU and GPU (issue #742, 2026-11-06)

Issue #742 asked for a compiled kernel subset for per-pixel procedures: a
compiled CPU path first, then a GPU backend as a preview. This branch builds
both, with four gallery pieces, and gives a go/no-go. The design record is
[`docs/dev/design/KERNELS.md`](../docs/dev/design/KERNELS.md); the user guide
is [`docs/guides/kernels.md`](../docs/guides/kernels.md).

**Compared with #1044** (array-at-a-time operators, filed the same day by
fractal_clouds as the lighter alternative):

- The executor here *is* array-at-a-time: each instruction runs over a batch
  of 16 lanes.
- What #742 adds is the compiler that writes the array program from ordinary
  scalar PostScript. fractal_clouds' renderer compiled with no wavefront
  rewrite (below).
- #1044 stays useful as a user-facing buffer API for multi-pass work
  (erosion, reaction-diffusion), and could reuse the executor. It is not
  needed for per-pixel work.

### What was built

- **`kernelimage`** (`w h matrix proc ncomp`). `proc` is `i j -> c1..cn`, in
  a fresh dict per pixel, and the result is painted like an 8-bit
  `colorimage`.
- **`kernelcompile`**: `proc ncomp -> bool reason`.
- **`.kernelmode`**: `/auto`, `/interpret` or `/strict`.
- **`.kernelstats`**: a test hook.
- **`--gpu-preview`**, with the `gpu` cargo feature.

The code is in `src/kernel/`:

- **`compile.rs`, an abstract interpreter over a symbolic stack.**
  - Stack ops, names and calls are resolved at compile time.
  - Names and stack slots are SSA values, merged at joins with merge
    registers.
  - Loops reach a fixpoint over the slots they carry.
  - Short constant loops are unrolled, and so are loops that change the stack
    depth.
  - Names defined on some paths only carry a per-lane flag that a read
    asserts.
  - A branch that does not compile becomes an abort for its lanes.
- **`ir.rs`**: a structured register IR. Registers are I64 (exact integers)
  or F64; mixed values are F64 with an integer tag.
- **`exec.rs`, the lane-batched executor.**
  - It works on 16-lane batches with masks, on all cores.
  - A lane that would error is *retired*.
  - It uses the interpreter's own expressions (`fmath`, the noise
    functions), so the bytes are the same.
- **`regalloc.rs`**: linear-scan allocation over structured live intervals.
  fractal_clouds' `fc-pixel` went from 70,097 to 515 registers.
- **`check.rs`**: a debug-build validator of register storage kinds.
- **`gpu.rs`**: WGSL lowering, run through wgpu, in f32.
- **`mod.rs`**: the operators, the threaded runner, the `--bands` row range,
  and two interpreter drivers: one for the whole image, one for the retired
  pixels only.

### Correctness

The compiled path is byte-identical to the interpreter, errors included.
Every test renders twice, under `/strict` and under `/interpret`.

**`tests/kernels.rs` (21 tests):**

- arithmetic and integer promotion;
- loops and exits;
- stack-carried loop state;
- dict scopes;
- tables, tuples and local arrays;
- the noise operators;
- 64-bit hashes past 2^53;
- error parity;
- limits;
- thread-count and `--bands` byte identity;
- a seeded differential fuzz over random programs, hash-shaped expressions
  included.

**Mutation checks** (each deliberately broken executor caught by the suite):

- the PostScript `round` rule;
- an off-by-one in `for`;
- a store that ignores the mask;
- a 1e-12 error in `sub`;
- a dropped int-to-real conversion, which the kind validator rejects
  outright.

**Whole renders, compared with `/interpret`:**

- the three benches;
- the fractal_clouds `mountains` frame;
- Mirror Garden at full size.

**Bugs the testing caught on the way:**

- A rollback after a failed unroll attempt dropped instructions, because
  `compile_if` did not restore the enclosing block on an error path. The
  differential test hung, which is how it was found.
- Integers held as exact f64s cannot hold fractal_clouds' 31-bit hash
  products, which reach about 2^58. That forced the I64 register kind.
- My own art had two bugs of its own (an overflowing hash, and `fill`/`ln`
  used as variable names). Both paths agreed on the real error each time.

**Robustness for untrusted programs (pscat-mcp):**

- `--max-time` and `--max-steps` are checked between batches and every 256
  loop trips. A runaway kernel `{ } loop` ends with `TimeExceeded`, and that
  is tested.
- Buffers and table snapshots go through `check_alloc`.
- The drivers' helper operators are not in systemdict, and they validate
  their state dict as untrusted input.

**Codex adversarial review** (2026-11-06, verdict "needs attention").

Ten findings, all valid. Each has a regression test in `tests/kernels.rs`
except item 7, which is GPU-only and was checked by hand.

1. **Alias cycles.** A name-alias cycle (`/a /a cvx def`) overflowed the Rust
   stack. Alias hops now count against the inline-depth guard.
2. **Dimensions.** `w × ncomp` could overflow before validation. Every size
   product is now checked up front, capped at 4 GiB, as a `limitcheck`.
3. **Compile work.** Exponential inlining with no emitted IR escaped the
   limits. There is now a compile work budget, charged as steps, with time
   and step polling during compilation. A limit hit there is that limit's
   error, never swallowed by a branch fallback or an unroll retry.
4. **Worker memory.** Worker register files and local arrays bypassed
   `--max-memory`. They are now reserved through `check_alloc` for all workers
   before any is spawned. When they do not fit, the interpreter does the image
   instead, so the program never sees an error the interpreter would not
   raise.
5. **Side effects in fallback.** A retired pixel's interpreted run could have
   side effects (`store` in an uncompiled branch) that compiled pixels did not
   see. Per-pixel fallback is now used only when the kernel has no uncompiled
   branch (compiled code has no side effects). Otherwise the whole image is
   interpreted.
6. **Exact integers in mixed values.** An integer meeting a mixed value (an
   integer on some lanes, a real on others) was rounded past 2^53. It now
   converts exactly, retiring the lane if it cannot, in arithmetic and in
   comparisons.
7. **GPU deadline.** The GPU had none. A render under any time or step limit
   now stays on the CPU, and in the shader one loop budget covers all of a
   pixel's loops, nested ones included. A runaway kernel stops in 1 s on the
   GPU, and with `--max-time 1` on the CPU.
8. **Row checks.** `T i get pop` on a two-level table skipped the range check.
   The row index is now checked when the row is fetched.
9. **`bind`** was treated as a no-op. Inside a kernel it is now refused, and
   the procedure runs in the interpreter.
10. **`count`** saw only the kernel's own stack. It is now refused the same
   way.

None of the fixes changed the gallery renders' bytes or speed.

**Second Codex round.** The adversarial mode broke off twice on its own
output format; the plain review mode completed. Six more findings, all valid
and all fixed, each with a regression test. Mutation checks (the fix removed,
the test must fail) were run for items 1, 4 and 6.

1. **Exactness at 2^53.** An integer converted to an f64 was checked *after*
   conversion, so 2^53 + 1 (which rounds to 2^53) passed. Integers are now
   checked as integers, 2^53 inclusive. f64 results are checked strictly below
   2^53, since 2^53 may itself be a rounded 2^53 + 1. My regression test for
   the first round had hidden this: it used 2 components (a `rangecheck` in
   both modes) and painted into one device pixel.
2. **Symbolic stack.** `n copy` could double the compile-time stack without
   charge. Each copy is now work, and the stack is capped at 4096 entries,
   past which the interpreter runs the procedure.
3. **Fallback allocations.** The interpreter fallback's row buffer and its
   pixel list now go through `check_alloc`. Its test shows the right error and
   no crash, but it cannot show the allocation was avoided: without the fix,
   the zeroed row is allocated lazily and the interpreter's own memory check
   still fires afterwards.
4. **Merged arrays.** Merging arrays at a join lost aliasing. Merged arrays,
   and the arrays merged, are now frozen: a later `put` refuses compilation.
5. **Row identity.** `eq` on two run-time table rows compared index
   expressions. It is now undecidable at compile time, so refused.
6. **Row types.** `length`, `aload` and `forall` on a null row, and `aload`
   on a string row, now raise the interpreter's error: a row check retires the
   lane. Every `kernelcompile` and refused compile also charges its work as
   steps.

### Measurements

Release build, Apple M4 (4 performance + 6 efficiency cores). Wall time
includes process start.

**Benches** (`scripts/bench_kernels.sh`, 480×320, median of 5):

| bench | plain, 1 core | `--bands 8` | kernel, 1 thread | kernel, all cores | vs plain | vs bands 8 |
|---|---:|---:|---:|---:|---:|---:|
| mandel (escape time, 200 iterations) | 4.77 s | 1.39 s | 0.216 s | 0.064 s | 75× | 22× |
| relief (noise heights, three samples a pixel) | 0.82 s | 0.24 s | 0.167 s | 0.055 s | 15× | 4.4× |
| spheres (ray tracer, 3 bounces) | 1.67 s | 0.50 s | 0.056 s | 0.030 s | 55× | 16× |

Relief gains least: most of its time is already in native `fbm2`/`ridged2`.

**fractal_clouds, the program that filed #742.** The `mountains` scene's
whole `fc-pixel` compiles: terrain, erosion lookups, sky, clouds and tone
mapping, 48k IR instructions. The test was a 160×100 frame, AA off, via a
`kernelimage` driver; the result is byte-identical to `/interpret`.

| | `kernelimage` call |
|---|---:|
| interpreter | 17.95 s |
| compiled, 1 thread | 1.24 s (14.5×) |
| compiled, all cores | 0.29 s (61×) |

The scene's own bakes (erosion and forest maps, about 12 s) are not
per-pixel and stay interpreted. They now dominate a frame at this size.

**Batch width.** The fractal_clouds frame and the benches, 1 thread,
compiled run only:

| lanes | fractal_clouds | mandel | relief | spheres |
|---|---:|---:|---:|---:|
| 64 | 3219 ms | 385 | 141 | 62 |
| 32 | 1495 | 207 | 132 | 35 |
| **16** | **1237** | **183** | **130** | **31** |
| 8 | 1238 | 206 | 134 | 35 |

16 lanes wins on all four. Narrower batches waste fewer lanes where rays
diverge, and keep the register file (registers × lanes × 8 bytes) in cache.

**Gallery pieces** (full size, `--dpi 144`; the interpreter column is the same
file under `/interpret .kernelmode`):

| piece | grid | interpreter | kernelimage (10 cores) | GPU dispatch |
|---|---|---:|---:|---:|
| Mirror Garden (ray tracer) | 1440×960, 4 rays/pixel | 149 s | 0.9 s | 17–23 ms |
| Mandelbulb (ray-marched fractal) | 1200×1200, 4 rays/pixel | 345 s | 4.9 s | 66 ms |
| Dreaming Network (CPPN inference) | 1200×1200 | 349 s | 3.9 s | 86 ms |
| Mochi in Oils (Kuwahara filter, 36 MB photo) | 1200×1600 | 273 s | 2.7 s | 99 ms |

The interpreted renders match the compiled ones byte for byte: 70–165× faster on all cores, as the same pictures.

**GPU preview** (`--gpu-preview`, f32, Metal on the M4's 10-core GPU):

- **Speed.** Dispatch plus readback is **23–68× faster than the CPU path on
  all ten cores**: Mandelbulb 66 ms against 4.6 s, Dreaming Network 86 ms
  against 4.0 s.
- **Fixed costs.**
  - Device setup is 8–250 ms.
  - Shader compile is 75–660 ms cold, and 17–25 ms once Metal has cached it.
  - The small benches are no faster end to end: those fixed costs dominate.
- **Fidelity.** f32 against the exact f64 bytes:
  - Dreaming Network differs by at most 1 level in 255.
  - Mandelbulb and Mochi in Oils differ by a mean of 0.00 levels.
  - Mirror Garden differs by more than 16 levels on 2.7% of pixels. Its
    integer hash for shadow jitter is not exact in f32, though the picture
    looks the same.
  - The Mandelbrot bench differs on 2.5% of pixels: chaotic iteration near the
    set's boundary.
- **Packaging.** The cost is an optional dependency tree (wgpu 30, MSRV 1.87,
  MIT/Apache), off by default and never on wasm.

### Large kernels (#1064)

**Setup.** fractal_clouds, 160×100 frames through `kernelimage`, release build,
10 threads, machine load about 4.5, three interleaved rounds. "Before" is
`feature/compute-kernels` at 978983f. Every render is byte-identical to
`/interpret`.

**`mountains`.**

| | instructions | registers | compile | `kernelimage` run |
|---|---:|---:|---:|---:|
| before | 48,462 (44,326 in some runs) | 515 | 81–86 ms | 195–220 ms |
| after, default (`/auto`: DCE, no sharing) | 34,801 | 56 | 87–90 ms | 200–210 ms |
| after, `/large .kernelshare` | 13,791 (61 subroutines) | 285 | 40–41 ms | 210–213 ms |

**`alpine_forest`.** The procedure, by `kernelcompile`:

| | instructions | soft branches | compile |
|---|---:|---|---:|
| before | 130,724 | 2 "too large", 6 "defined on some paths only", sometimes 1 "put into an array that a branch chose" | 1.06 s |
| after, default | 103,173 | the 6 "defined on some paths only" | 1.2 s |
| after, `/large` | 24,176 (98 subroutines) | the same 6 | 0.19 s |

**The whole image goes to the interpreter.** In every case above, the frame
still runs in the interpreter (about 32 s). 11,196 of the 16,000 pixels take
one of the remaining soft branches.

**Checking the compiled pixels.** A debug build that keeps the kernel's own
pixels instead of falling back checked the other 4,804. All of them are equal
to `/interpret`:

- under `/auto`;
- under `/large`;
- under `/all`, which made 195 subroutines.

**Varying sizes.** The frozen-array set of the merged `[ ]` arrays held bare
addresses. A new array allocated where a freed frozen one had been was refused
a `put`, by chance. That is why the "before" sizes vary from run to run. It is
fixed on this branch, and the sizes are now the same on every run.

### Go/no-go

- **CPU kernels: go.**
  - The gain is 15–165× over the interpreter on real per-pixel programs.
  - Against the best band-parallel interpreted render (`--bands 8`) it is
    4.4–22×. The low end is the relief bench, whose time is mostly in native
    noise operators that were already fast.
  - There is no change to output (byte-identical).
  - There is no new dependency.
  - The program that asked for it compiles unmodified apart from the call
    site.
- **GPU: worth keeping as an opt-in preview, not more.**
  - It is another 23–68× on heavy kernels, which is what an interactive
    preview of a 30-core-minute fractal_clouds frame needs.
  - But it is f32, not bit-exact.
  - It pays a fixed 25–700 ms per call, so it loses on small images.
  - It adds a large optional dependency.
  - Final renders should stay on the CPU path.

### Deferred, and follow-ups to file

- **Names defined only inside a loop and read after it** do not compile
  (Mandelbulb works around it with an initial `def`). Loop-defined names
  could carry a defined-flag the way branch-defined names do.
- **`rand` and `srand` in a kernel** (done in #1063, see `KERNELS.md`).
  fractal_clouds' `PixSeed` reseeds per
  pixel, so a per-lane random state seeded inside the kernel would be pure.
- **Kernels past the 200k-instruction cap** (#1064, done): a counting bug,
  dead-code elimination, and shared subroutines. See the design note.
  `alpine_forest`'s `fc-pixel` now compiles with no size-cap branch. It still
  has six "defined on some paths only" branches. These are names that a loop
  defines on some of its paths only (at its `exit`, or in a branch of its
  body, as `fc-tree-cell` does with `FcIH`) and that are read after the loop
  behind a guard. That is the next item. While a lane takes one of those
  branches, the whole image runs in the interpreter, so forest frames are
  not yet faster.
- **fractal_clouds' depth-edge antialiasing** reads neighbouring pixels'
  depth keys (carried state). It needs a two-pass form: one kernel for keys,
  a second for colour.
- **`--gpu-preview` and pscat-mcp.** It is CLI only, and the MCP server could
  pass it through. On the GPU, f64 could be emulated (double-float) for
  integer hashes.
- **#1044's buffer API** for multi-pass simulations, on top of this executor.
- **A `kernelimage` shim in `lib/`** for Ghostscript. The gallery pieces carry
  their own ten-line fallback.
