# Compiled per-pixel kernels (`kernelimage`): design

Status: in progress on branch `feature/compute-kernels`. Tracks #742, a
compiled kernel subset with CPU first and the GPU as a preview backend.
Compared below with #1044, array-at-a-time operators. Both were raised by
ArtColony/fractal_clouds.

## The problem

Per-pixel PostScript is bound by per-token cost. After #863 and #864, a token
still costs 5–20 ns, whatever it does:

- a stack shuffle (`3 index`, `4 1 roll`) costs about as much as the
  arithmetic it feeds;
- a name lookup of a scratch variable (`/u exch def` ... `u`) costs a dict
  probe each time;
- a bound library procedure call costs a frame push and pop.

The ray-marched landscape renderer in fractal_clouds spends 30–60
core-minutes on a 1280×800 frame. Its profile is about 1.2 billion `mul`,
0.94 billion `add`, 0.81 billion `def`, 0.55 billion `exch`, and so on. It is
almost all pure arithmetic over a few tables, plus control flow. Native noise
(#734) and `--bands` (#736) are already in. Rewriting the scalar PostScript
stopped paying off at about 5% (fractal_clouds#94).

The same shape shows up in:

- ray tracers;
- fractals (escape-time, distance-estimated ray marching);
- image filters over a decoded photo;
- small neural networks evaluated per pixel (CPPN, neural fields), that is,
  "model inference" inside a PostScript program;
- shaded-relief maps (`gallery/basin_and_range.ps`).

## Alternatives considered

| | #742 compiled kernel subset (this branch) | #1044 array-at-a-time operators |
|---|---|---|
| What the user writes | an ordinary scalar procedure `{ i j -> r g b }` | whole-array expressions over packed buffers |
| Rewrite of existing code | none for the subset: a `colorimage` loop body becomes a kernel by moving the loop out | a wavefront rewrite (masks, `select`, compaction for divergent ray marches) |
| Stack and name traffic | gone at compile time: the compiler resolves `dup`, `index`, `roll` and `def` into registers | still paid once per array op; amortised over the batch |
| Divergent control flow | masked lanes, chosen by the engine | by hand, with masks and compaction |
| Engine cost | a compiler (abstract interpreter) plus an executor | a new object type plus about 40 elementwise operators |
| GPU path | the same IR lowers to one shader per kernel | one dispatch per array op, bandwidth-bound unless fused |

#1044's internals are what the kernel executor does anyway. It runs each IR
instruction over a batch of lanes, so per-instruction overhead is amortised
exactly as an array op would amortise it. The difference is who writes the
array program: in #742 the compiler writes it from scalar code. This branch
implements #742. #1044 stays open as a complementary user-facing API (buffers
for multi-pass work such as reaction-diffusion or erosion); it could later
reuse the executor.

## Operator surface

```
width height matrix proc ncomp kernelimage -
```

- `width height` give the sample grid and `matrix` is the ImageMatrix, as for
  `image`.
- `ncomp` is 1 (gray), 3 (RGB) or 4 (CMYK).
- `proc` is called as `i j proc c1 ... cn`:
  - `i` (column, 0 at the left) and `j` (row, 0 at the *first* data row, as in
    `image` data order) are integers;
  - each component is a number, clamped to [0, 1] and quantised to 8 bits by
    the rule below. Quantisation is one Rust function used by both paths.
- The result is painted exactly like an 8-bit `colorimage` with that
  matrix: the buffer goes through the existing `ImageCtx` rasteriser, so PNG,
  SVG, PDF, `--bands` and clipping behave as for `image`.

pscat extensions, probed with `where` (#646 discussion):

| operator | stack | what |
|---|---|---|
| `kernelimage` | `w h matrix proc ncomp -` | the above |
| `kernelcompile` | `proc ninputs noutputs - bool string` | compile only: does the proc compile, and if not, why |
| `.kernelmode` | `name -` | `/auto` (default), `/interpret` (always the fallback), `/strict` (an uncompilable proc is an error, not a fallback) |

The fallback a portable program writes for other interpreters is a
`colorimage` loop that calls the same proc in a fresh dict (the
`kernelimage` shim in `lib/kernel.ps`).

### Per-pixel semantics: a fresh local dict

Every pixel runs as if by `8 dict begin i j proc end`.

- A `def` writes the pixel's own local dict.
- A read of a name the pixel has not yet defined resolves through the dict
  stack *as it stood when `kernelimage` was called*.
- Nothing a pixel defines is visible to the next pixel or after the call.

So carried state between pixels, like basin_and_range's `/left` and `prev`
row, is impossible by construction. Both paths give the same answer for a
read before a write. A `def` under one branch of an `if` starts its register
from the snapshot value.

The fallback path is literally that: per pixel, `dict begin i j proc end`, run
by the interpreter. It is also the reference the compiled path is tested
against.

## The compiled subset

The compiler is an abstract interpreter over the procedure. It keeps a
*symbolic* operand stack whose entries are compile-time constants or IR
registers.

| construct | compiled as |
|---|---|
| integer, real and boolean literals | constants |
| `dup exch pop copy index roll` (constant operands), `mark counttomark cleartomark` | compile-time stack moves: zero run-time cost |
| `/n v def`, `/n exch def` | a register in the innermost compile-time scope |
| `N dict begin ... end` | a compile-time scope |
| an executable name bound to a number or boolean | a captured constant |
| an executable name bound to a procedure, `exec` of a known proc | inlined (depth-limited; recursion falls back) |
| an operator, bound or looked up | its IR instruction |
| `add sub mul div idiv mod neg abs ceiling floor round truncate cvi cvr sqrt exp ln log sin cos atan min max` | arithmetic IR, PostScript promotion rules |
| `eq ne lt le gt ge and or xor not bitshift true false` | compare and logic IR |
| `if ifelse` with literal procs | structured IR |
| `for repeat loop exit` | structured loop IR; bounds may be run-time values |
| `[ ... ]` of constant length, `aload astore length`, constant-index `get`/`put` on such an array | scalarised: an array of registers |
| `N array` (N constant) with run-time `get`/`put` | a per-lane local array |
| `get` on a captured array or string of numbers with a run-time index | a gather from a snapshot table |
| `get` on a captured array of equal-length numeric arrays | a row reference; then `aload` or a constant `get` gathers |
| `gnoise2 gnoise3 vnoise2 vnoise3 fbm2 fbm3 vfbm2 vfbm3 ridged2 ridged3 gnoise2d` | calls to the same Rust functions, with the table snapshotted at compile time |
| `srand`, then `rand rrand` | the LCG on a per-lane random state (#1063, below) |

Anything else is not compilable: `put` into a captured composite, `store`,
`rand` before the pixel's `srand`, strings, dicts as values, graphics, I/O,
`stopped`, and so on.
The proc then runs through the fallback (one stderr note per call site), or
raises an error under `/strict`.

### Random numbers (#1063)

A pixel that calls `srand` before any `rand` draws only from its own seed,
so it is still a pure function of `i` and `j`.

- **State.** The random state is a binding in the pixel's own scope, under
  an id no name can have. So it merges at joins and is carried round loops
  like any name. It is an integer: -1 before the pixel's first `srand`
  (no real state is negative), then the LCG state.
- **`srand`** masks its integer operand to 31 bits. Under `--sweep-seed`
  it sets the forced seed instead. An `rrand` value coming back is a
  restore: its lanes retire, so the interpreter does it.
- **`rand`** is the interpreter's step, `(s * 1103515245 + 12345) &
  0x7fffffff`, as I64 ops. When no path has seeded, the procedure is
  refused. When only some have, the lanes still at -1 retire.
- **Afterwards.** The state an interpreted render leaves is the one the last
  seeding pixel in raster order left. The run records it per computed row.
  Rows it did not compute (other `--bands` bands) are run again, from the
  last row back, until a pixel that seeded is found. Under an override, a
  pixel that seeded is one whose `srand` fired.
- **Retired pixels.** A retired pixel's `rand` may read the state its
  compiled predecessor left, which the interpreter never saw. So a seeding
  kernel with any retired pixel runs the whole image in the interpreter,
  from the untouched starting state.
- **GPU.** A kernel with `srand` stays on the CPU: the LCG's products do
  not fit an i32.

### Numbers and errors

- A lane value is an f64, with an "is integer" bit. The bit is static when
  the compiler knows the type, and per lane where branches merge different
  types (`dup 0 lt { pop 0 } if` on a real).
- Integers within ±2^53 add, subtract and multiply to the same bits in f64 as
  in i64.
- Every operation is the interpreter's own expression. Transcendentals call
  the identical `fmath` expressions per lane: `sin(x.to_radians())`, atan's
  degrees and +360 fix-up, and `exp` through `powf`. There is no fused
  multiply-add, no fast-math, no cross-lane reduction, and nothing depends on
  thread count.

**Error rule.** The compiled path never raises a PostScript error itself. If
any lane would raise one, the kernel aborts, and the whole image reruns
through the interpreter fallback, which raises the real error with the real
operand stack and error command. That covers:

- `rangecheck`, `typecheck` (a real used as an index, an int-only op given a
  real), `undefinedresult`;
- `sqrt`, `ln` or `log` of a negative or zero;
- an integer result past ±2^53.

Errors are rare in finished art, so this keeps the semantics exact without
modelling error states.

## Executor

```
proc --compile--> IR (structured: Seq / If / Loop / Break)
                    |
        +-----------+-------------+
        |                         |
 lane-batched CPU executor    WGSL codegen (feature "gpu")
 (exact, default)             (f32 preview, opt-in)
```

The CPU executor:

- It interprets the IR over a **batch** of lanes, one lane per pixel, a run
  of up to 256 pixels of one row. Each instruction is a tight loop over the
  batch's registers (`[f64; LANES]`), which LLVM vectorises to NEON or AVX.
  Dispatch costs once per instruction per batch, not once per pixel.
- `If` evaluates a mask. When all lanes agree, it runs one branch, which is
  the coherent fast path. Otherwise it runs both, with masked register
  writes.
- `Loop` runs while any lane is still active. `exit` clears a lane's bit.
  `for` with uniform bounds needs no mask.
- Rows are spread over threads (`std::thread::scope`, workers equal to the
  core count). The compiled kernel and its snapshot tables are plain `Send`
  data; the `Interp` is not touched off its own thread. On wasm32 there is
  one thread.
- Under `--bands N`, each band computes only the sample rows whose footprint
  reaches its band (plus one row of margin), on one thread. An unaware
  program gets the band speedup too.

### Limits

pscat-mcp runs untrusted programs under `--max-time`/`--max-steps`/
`--max-memory`.

- The sample buffer is allocated through `check_alloc`.
- Workers check the time deadline once per batch and every 4096 loop
  iterations, so a kernel `{ } loop` with a data-dependent `exit` still ends
  with `TimeExceeded`.
- Steps are charged per pixel as the compiled instruction count executed, a
  documented proxy. `--max-steps` therefore still bounds a kernel.

## GPU preview backend

Behind the cargo feature `gpu` (off by default, never on wasm), and enabled
at run time by `--gpu-preview`:

- The same IR lowers to WGSL. Integers become i32, reals f32, and noise ops
  are ports of the same formulas over the snapshot permutation table.
- It runs through wgpu, which uses Metal on macOS and Vulkan or DX12
  elsewhere.
- It is approximate (f32), so it is never the default, and output is never
  claimed bit-exact. Final renders stay on the CPU path.
- Under `--max-time`/`--max-steps` (pscat-mcp's `gpu_preview`, #1065) the
  limits are checked before each dispatch of 65536 pixels, and a dispatch is
  charged the kernel's straight-line size per pixel in steps. Loop trips are
  capped at 2^20 per pixel, not counted, so a runaway loop gives a capped
  image rather than an error unless the capped work runs past a limit.

Its value is to be measured, not assumed. "The GPU isn't worth it" is a valid
outcome of this work.

## Milestones and gates

- **M0.** A benchmark harness and baselines: interpreter plain and with
  `--bands 8`, on a basin_and_range-style relief map, a Mandelbrot and a
  sphere tracer.
- **M1.** The compiler, a scalar executor, the fallback, `.kernelmode`, and
  differential tests (kernel vs forced fallback, bytes and errors).
- **M2.** The lane-batched executor.
- **M3.** Threads and bands.
- **M4.** The GPU preview.
- **M5.** Gallery pieces, docs and the go/no-go.

Every speedup is reported against both one-core plain and `--bands 8`.

## Measurements

Filled in as the milestones land: see `notes/742-compiled-kernels.md`.
