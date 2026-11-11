## #1102: `kernelarray`, a compiled procedure's values as numbers (2026-11-11)

fractal_clouds bakes its height, erosion and forest grids before it draws.
Each bake is a nested `for` loop that ends in `put`, and the bakes stayed
interpreted. On a cache miss they were about 16 s of a 23 s preview render.

```
w h proc n kernelarray array          % cell (i, j)'s values at (j*w + i)*n
w h proc n kernelarrays a1 .. an      % one array of w*h per value
```

### Design

- The compiler, executor, threads, soft branches and retired lanes are all
  `kernelimage`'s. Only the ends differ:
  - `compile` takes `Out { n, typed }`. A typed kernel keeps each output as
    it is (an integer's I64 bits, a real's or a Dyn value's f64) and appends
    its `NT::tag`. `kernelimage`'s code is unchanged.
  - `Machine::cell` reads a lane's values.
  - `run` is generic over what a pixel writes.
- One driver, `CELL_DRIVER` (`.kanext`/`.kaput`/`.kadone`), is both the
  `/interpret` reference and the path for retired cells. A cell therefore
  sees the same operand stack and execution-stack depth either way.
- A cell that leaves with `exit` raises `invalidexit`.
- Every row is computed, under `--bands` too. There is no GPU path.
- `kernelfill` (fill an existing array) is left out. A procedure that read
  its own target would see it change under the interpreter, but read a
  snapshot when compiled. fractal_clouds does not need it: the pass that
  rewrites an array in place stays interpreted.

### A compiler bug that bytes hid

`kernelarray` is the first operator that shows a kernel's raw values. A
5,000-case fuzz, and separately the independent review, found that a Dyn
value's integer lanes could hold **-0.0**: `0 -2 mul` or `0 neg`, computed
in f64. Read later as a real, the sign leaked: `cvr`, `2.5 mul` and `atan`
gave -0.0 where the interpreter's integer 0 gives 0.0.

It reached `kernelimage`'s pixels too, though rarely: `0 -1 exp` is inf
(255) in the interpreter but -inf (0) compiled.

The fix: a Dyn `mul` checks its result with `BinOp::IntZeroCheck` in place
of `Ins::CheckInt`. A Dyn `neg` is `BinOp::NegIntZero`. Both add 0.0 on
integer lanes only, and select the value untouched on real ones.

- **Instruction count.** Fused this way the fix adds no instruction to a
  `mul`, and a `neg` stays one instruction. A first version added three
  instructions after each and cost fractal_clouds' scenes up to 5.7% more
  steps.
- **Real lanes are selected, not added to.** A version that added -0.0 on
  real lanes changed a NaN's sign after `neg`: Apple silicon's arithmetic
  returns the default NaN, while `neg` only flips the sign bit. The fuzz
  caught it.
- **Test.** `a_dyn_integer_zero_has_no_sign` fails without the fix, and it
  covers a real lane's -0.0 and NaN.

### Checks

- `tests/kernels.rs`, `kernelarray` section:
  - types and bits, compared as type plus `to_bits`;
  - edge values: inf, NaN, -0.0, integers past 2^53;
  - errors and operand stacks;
  - soft branches, run per cell and as the whole grid;
  - `srand` and the random state afterwards;
  - thread count and `--bands`;
  - a later pass reading an earlier grid;
  - a fuzz with edge leaves. `KERNEL_FUZZ_CASES=20000` passed in release
    with four seeds (`KERNEL_FUZZ_SEED`), 80,000 programs in all.
- `tests/kernel_shim.rs`: the shim's `kernelarray` gives the native one's
  values and error names.
- `kernelimage` after the change: the gallery's four pieces, mountains and
  golden_coast at 640×400, and all 9 fractal_clouds scenes at 160×100 are
  byte-identical to build2 (46ad11b), with identical step counts.

### Speed

The 512² erosion sample in fractal_clouds' `fc_erode.ps` (`fc-height-proc`
per texel) went from 4,346 ms interpreted to 25 ms with `kernelarray`, on an
M4 with other load. The mountains render is byte-identical.

### Open

- The shim's errors leave its own operands behind, and a cell that pops its
  own mark is `unmatchedmark` there but `rangecheck` natively. The
  `kernelimage` shim has the same difference.
- `kernel_notes` dedupes on the reason text alone, so a refusal note for one
  operator hides the same note for the other.
