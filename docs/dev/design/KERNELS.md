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
| `kernelarray` | `w h proc n - array` | the values as numbers (#1102, below) |
| `kernelarrays` | `w h proc n - a1 .. an` | the same, one array per value |
| `kernelcompile` | `proc ninputs noutputs - bool string` | compile only: does the proc compile, and if not, why |
| `.kernelmode` | `name -` | `/auto` (default), `/interpret` (always the fallback), `/strict` (an uncompilable proc is an error, not a fallback) |
| `.kernelshare` | `name -` | `/auto` (default: share procedure bodies as subroutines only when inlining makes the kernel too large), `/never`, `/large`, `/all` (see "Large kernels") |

The fallback a portable program writes for other interpreters is a
`colorimage` loop that calls the same proc in a fresh dict (the
`kernelimage` shim in `lib/kernel.ps`).

### `kernelarray`: typed outputs (#1102)

`kernelarray` keeps the compiler, the executor, its batches and threads,
the soft branches and the retired lanes. Only the two ends differ:

- **Outputs.** `compile` takes an `Out { n, typed }`. For `kernelimage` each
  output is converted to an f64 (`as_f`) and quantised, as before: its code
  is unchanged. A typed kernel (`Kernel::typed`) keeps each value as it is,
  and `outputs` holds the `n` values and then a tag for each, `NT::tag`'s:
  1.0 for an integer (an I64 register, or an immediate), 0.0 for a real, or
  the F64 tag register of a Dyn value. The tags sit in `outputs`, so the
  allocator, dead-code removal and the kind check treat them as the roots
  the outputs are. `Machine::cell` reads a lane's values as `Num`s; a Dyn
  integer is exact in its f64 (at most 2^53).
- **The interpreter path.** `CELL_DRIVER` is one driver for the whole grid
  and for the cells a compiled run retired: `.kanext` pushes `mark i j` for
  the next cell (raster order, or the next retired one), and `.kaput` pops
  the values back to the mark (a `typecheck` for a non-number, a
  `rangecheck` for a wrong count) into the result. Both runs therefore show
  a cell the same operand stack and execution-stack depth. A cell that
  leaves with `exit` ends the driver's loop unfinished: `.kadone` raises
  `invalidexit` for the whole grid, or, among retired cells, puts the
  caller's operands back and runs the whole grid instead. Whether retired
  cells may run alone is decided as for `kernelimage` (`srand`, and soft
  branches with side effects in reach).
- Every row is computed, under `--bands` too: the array is the result. There
  is no GPU path, since f32 is not exact.
- **NaNs.** Every NaN is stored as `f64::NAN` (`canon_nan`, in
  `Machine::cell` and `.kaput`). A NaN's sign and payload differ between
  paths: x86 passes on one operand's NaN, chosen by the compiler's operand
  order, and Apple silicon returns its default NaN. Nothing but the stored
  bits can tell NaNs apart.
- **Constants compare by their bits** (`impl PartialEq for Op`). Float `==`
  let a branch join keep 0.0 for -0.0 on every lane (Codex review 3).

`run` is generic over what a pixel writes (bytes or `Num`s), so
`kernelimage` monomorphises back to its own loop.

A form that fills an existing array (`kernelfill`) is left out: a
procedure that read the array it fills would see it change under the
interpreter, but compiled it reads a snapshot.

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
A name defined on some paths only, under one branch of an `if` or inside a
loop that may run no trips or exit before its `def`, carries a per-lane
"defined" flag. A read of it asserts the flag: a pixel that would read it
undefined sends the image to the interpreter, which raises the real error.

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
| `get` on a captured array of booleans with a run-time index | a gather from a 0/1 table, a boolean (#1092) |
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

**Error rule.** The compiled path never raises a PostScript error itself. A
lane that would raise one retires, and the retired pixels run in the
interpreter, one by one in raster order after the compiled ones (the whole
image, when a soft branch below rules that out). The interpreter raises the
real error with the real operand stack and error command. That covers:

- `rangecheck`, `typecheck` (a real used as an index, an int-only op given a
  real), `undefinedresult`;
- `sqrt`, `ln` or `log` of a negative or zero;
- an integer result past ±2^53.

Errors are rare in finished art, so this keeps the semantics exact without
modelling error states.

### Soft branches: per pixel or the whole image (#1093)

A branch of an `if` that does not compile becomes an `Abort` (a *soft*
branch): the lanes that take it retire. Pixels compiled from the snapshot
are exact only if no interpreted pixel changes anything they read, so the
retired pixels run one by one after the compiled ones only when the
procedure is *pure* (`pure.rs`). Otherwise the whole image runs in the
interpreter.

- **What is scanned.** Not the soft branches alone. A retired pixel runs the
  whole procedure again, and past its soft branch it runs code the compiler
  checked only for the paths it compiled: a soft branch can rebind a name
  that the compiled rest then writes through (`/buf G def`, then `buf 0 x
  put` on a `buf` that is a local array on every compiled path). So the
  scan covers the universe of the procedure, everything it can reach: every
  element of every array or procedure reached, every key and value of every
  dictionary reached, the binding now of every executable name reached, and
  the operand stack under the call (a pixel can `index` into it).
- **Why that is enough.** A pure pixel cannot write any dictionary but its
  own, so a name resolves to its binding now or to a value the pixel
  defined. That value was on its operand stack: in the universe, or a fresh
  array or dictionary of such values. Nothing it can run turns data into
  code. So every object a pixel can execute is in the universe.
- **The rule.** Every operator in the universe is on an allowlist, and
  nothing in it is an executable string or file. The allowlist has stack
  operators, arithmetic, compare and logic, control (`if` to `forall`,
  `exec`, `exit`), `def`, `dict`, `array`, `>>`, the reads (`get`,
  `getinterval`, `length`, `aload`, `known`) and the noise operators. Left
  out are any write into an existing object (`put`, `putinterval`, `astore`,
  `copy`, `store`, `undef`, `cvs`, `bind`), the random state, graphics and
  files, the dictionary stack (`begin`, `end`: without them every `def`
  lands in the pixel's own dictionary), the execution stack, and data to
  code (`cvx`, `load`, `where`, `token`). `copy` is out because its
  composite form writes into its operand, and the stack form cannot be told
  apart from the text (a procedure's elements can be fetched with `get`).
  An operator not on the list is impure, whatever it does.
- **Under its operands.** Uncompiled code can reach under `i j`. The
  per-pixel driver shows a pixel `i mark i j` on the caller's operands, as
  the whole-image driver does, and pops what that driver pops, so a change
  there lasts the same way in both. Two things differ: a pixel that `exit`s
  out of the procedure (out of the row's `for` in the whole-image driver),
  and one that changes the `i` under its mark (the column that driver stores
  the pixel in). The per-pixel driver checks both after each pixel; on
  either, it puts the caller's operands back and runs the whole image
  instead. No pixel before changed anything else.
- **Errors.** A compiled pixel never raises an error: it retires. So the
  first pixel in raster order to error is a retired one, and the retired
  pixels run in raster order with the state the whole image would give them:
  the same error at the same pixel, with the same operand stack. The
  per-pixel driver also runs a pixel as many execution-stack frames deep as
  the whole-image driver does, so a deep recursion overflows in both or in
  neither.
- **`--bands`.** A band computes only its own rows, so a soft pixel in
  another band's rows would not run at all, though it can still raise an
  error or change the operand stack. With a soft branch under bands, a band
  that retires a pixel runs the whole image, as before #1093.
- **`srand`.** A seeding kernel (`Kernel::rand`) keeps the whole image for
  any retired pixel, as above; `srand` and `rand` are not on the allowlist.

The scan is conservative by construction: anything reachable counts, used
or not. fractal_clouds' `golden_coast` reaches `put`, `copy`, `cvx`,
`begin` and more (its cloud compiler and scratch tables are in reach), so
it still runs in the interpreter when a pixel takes a soft branch.

**Why pixels retire (#1092).** Each retired lane is counted under a reason:
the failing instruction's kind and operator (`exec::Why`: an integer
overflow, a zero divisor, the 2^53 check, a table index, ...), or, for
`Abort` and `Assert`, a site (`Kernel::sites`) naming what it stands for, the
operator, and the named procedures it was compiled in ("/x read where this
pixel has not defined it, in a > b > c"; a branch that did not compile, and
why). The note names the commonest reason and says whether the whole image
reruns; `PSCAT_KERNEL_TIMING` lists every reason with its count. A kernel's
soft branches are read off its finished code, so a branch is reported (and
makes the side-effect scan above decide) exactly when its `Abort` is in the
kernel.

### Large kernels: shared subroutines and dead code (#1064)

Every call is inlined, so a big program can pass the instruction cap
(400,000 since #1091; 200,000 before). fractal_clouds' `alpine_forest` reaches about 166k instructions
inlined (it hit the cap at 130k while a counting bug counted branch and
loop bodies twice). Two passes shrink large kernels.

**Dead-code elimination** (`opt.rs`) runs on every kernel. It removes pure
instructions whose results nothing needs, marked back from the roots in
one pass. A merge of a name that nothing reads after the join is a
typical case. An instruction that can retire a lane (a division, an
ordered compare, a checked integer op, a gather, a check) is a root, even
when its value is unused, because it stands for the interpreter's error.
The mountains kernel goes from 48k instructions to 35k.

**Shared subroutines.** A named procedure body can be compiled once, as a
subroutine (`Ins::Call`, `Kernel::subs`), and called from every place it
is used with the same symbolic context.

- **Recording.** While the body compiles, the compiler records what it
  reads of the state it entered with:
  - the stack entries it reaches;
  - the names it looks up or merges at a join;
  - the dictionary stack's shape.

  Names it only defines are outputs, not inputs.
- **The key.** That record, with constants by value, captured objects by
  identity, and registers numbered in order of first appearance with
  their kinds, is the subroutine's key.
- **Reuse.** The compiler is deterministic, and the body's compilation
  depends on the state only through the key. So a later call whose state
  has the same key would compile to the same code with its own registers.
  It calls the subroutine instead.
- **Making the subroutine.** The code is moved out of the body, and the
  outer registers it reads are renamed to parameters. At each call:
  1. the parameters are written;
  2. `Call` runs the body;
  3. each register the body leaves in the state is copied out at once into
     a fresh register, so the state after a call stays SSA.
- **What stays inlined.** A body stays inline when it:
  - writes a register from outside;
  - `exit`s a loop around it;
  - takes a `[ ]` array or a dict not begun, which it could change in place;
  - leaves an array or local array it built where the caller can see it.
- **Counting against the cap (#1091).** The cap counts the inline code
  plus each subroutine that code can still call, once. A subroutine made
  in code the compiler then discards (a branch rolled back to an abort,
  an earlier round of a loop's type fixpoint) stops counting:
  `rollback` and the fixpoint undo a trail of charged subroutines. It is
  kept, so a later call can reuse it, and that call charges it, and what
  it calls, again. A subroutine's body is pruned as it is made (DCE with
  its results, and the parameters of what it calls, as roots), and the
  cap counts what it keeps. The "too large" note gives the count and the
  cap. `PSCAT_KERNEL_TIMING` prints the count next to the size the
  finished code reaches, before the final DCE. A debug build checks that
  every subroutine reached is counted.
- **The cap** (`MAX_INSNS`, 400,000) bounds the kernel being compiled:
  its inline code and the subroutines that code reaches. An `Ins` is at
  most 128 bytes, so that is about 51 MB at the cap. fractal_clouds'
  evening_lake holds 299k instructions before the final DCE (82k
  after), golden_coast 236k (74k).
- **A second cap** (`MAX_SUB_MADE`, 1,600,000, 4 × `MAX_INSNS`, about
  205 MB) bounds every instruction a compile puts in subroutines, as
  made, discarded ones included: subroutines made in code later
  discarded stay, uncounted by `MAX_INSNS`, for reuse. Past it a body is
  not shared but refused, "the kernel is too large: N instructions made
  in subroutines, cap 1,600,000", so the branch aborts or the kernel
  falls back like any too-large one. fractal_clouds' scenes make at most
  274k (evening_lake); the kernel gallery pieces make none. `/auto` also keeps its
  first pass while it retries. The compiler's time is bounded by
  `MAX_WORK` and the program's time and step limits.
- **Executor and allocation.**
  - The executor runs a call like an `If` that every active lane takes.
  - The register allocator packs each body on its own, into a region past
    all its callers' regions, so a call clobbers nothing its caller keeps.
  - The GPU lowering inlines each call.

When it applies: `.kernelshare` (`/auto` by default) shares only when the
inlined kernel is too large. The compiler then compiles again, sharing every
body of 48 instructions or more from its second call on. If that kernel
is refused, or leaves more branches to the interpreter than the inlined
one, the inlined one stands (#1091): sharing lets branches that were
too large compile, and their code can push what follows over the cap. A
kernel under the cap gets exactly the inlined IR, so the mountains frame
is unchanged.
`/large` forces sharing, `/all` shares every named procedure from its
first call (a test mode), and `/never` turns it off.

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

- It interprets the IR over a **batch** of 16 lanes, one lane per pixel,
  an 8×2 tile of pixels (#1101; `PSCAT_KERNEL_TILE=WxH` overrides it for
  measuring). Neighbouring rows mostly take the same branches, so a tile runs
  about 5% fewer instructions than a run of 16 pixels of one row. Each
  instruction is a tight loop over the batch's registers (`[f64; LANES]`),
  which LLVM vectorises to NEON or AVX. Dispatch costs once per instruction
  per batch, not once per pixel.
- It runs a **lowered form** of the IR (`kernel/code.rs`, #1101), not the IR
  tree. That form is one flat array of 16-byte instructions, with nested
  blocks inline and subroutines before the body. Immediates sit in a constant
  pool, so integer ops still read them as `v as i64`. The lowering also
  makes these instructions:
  - add, sub and mul get their own opcodes;
  - a table read with constant tags and one good constant index becomes
    `Gather1`;
  - the wide, rare instructions keep their IR form in a side table.

  On large kernels the executor is bound by instructions per dispatch, not
  by memory: golden_coast (170k instructions, 4.4k registers) and mountains
  (96 registers) cost the same per dispatch. Its loops are written so LLVM
  vectorises them inside one large function:
  - operands are read in place, one loop per operand shape (register or
    immediate); by-value lane arrays get split into scalars;
  - a failing lane is found with one OR across the batch, and the mask is
    built only when some lane failed;
  - nothing selects a value for a failing lane, which is retired anyway.
- **Masked writes.** A write stores only the running lanes, as a blend, not
  a branch per lane. The exception is a write that starts its register's
  live interval (`Kernel::fresh`, from the register allocator). It may cover
  every lane, because on the lanes it skips the register holds nothing live.
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

### Instruction sets and vector math (#1100)

- **Tiers.** On x86_64 the executor's hot code (the block loop, `Gather1`,
  the `Ext` instructions and the vector math) is compiled three times:
  - the baseline, SSE2;
  - x86-64-v2, SSE4.1, which puts `floor` and `round` in vectors;
  - x86-64-v3, AVX2, with four f64s a vector.

  `exec::Isa::chosen` picks the best the CPU has, detected once, and
  `PSCAT_KERNEL_ISA` can force a lower one.
- **How each copy is built.** The `isa_dispatch!` macro wraps an
  `#[inline(always)]` body in one `#[inline(never)]` function per tier,
  each with its `#[target_feature]`. A nested block dispatches on
  `Machine::isa`, so it stays in its tier.
- **FMA is never enabled.** A fused multiply-add rounds once where the
  interpreter rounds twice. The emitted x86 assembly has no `vfmadd` in any
  kernel function, and the v3 copies use `ymm` registers throughout.
- **AVX-512 is not a tier.** Its target features need Rust 1.89, above the
  1.88 `rust-version`.
- **Vector math (`.kernelmath /fast`, `kernel/vmath.rs`).** `sin`, `cos`
  and `atan` are libm's algorithms:
  - `k_sin`, `k_cos`;
  - `rem_pio2`'s medium and small-multiple cases;
  - `atan`, `atan2`.

  The fast forms lower to `L::Ext`, the side table, not opcodes of their
  own: three more arms in the block loop's dispatch cost about 6% of the
  kernel time on fractal_clouds' scenes, run or not.
  Every path is computed on every lane and the lane's own one selected,
  using 64-bit integers only, so the loops vectorise. Lanes libm
  special-cases go to the exact scalar code, and `0 0 atan`'s error is
  decided there too.
  - **Rare paths.** Two paths are computed only when some lane of the batch
    needs them: the reduction's later rounds, and the no-reduction path
    below pi/4.
  - **Mode per run.** The mode is chosen per run in `Code::new`, never
    stored in a kernel, and `kernelarray` always runs exact.
  - **Exactness.** The tests compare every lane the vector code did against
    `fmath`, over 4M random inputs and the range boundaries, on arm64 and on
    x86_64.

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
