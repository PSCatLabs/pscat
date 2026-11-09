## Why pixels retire from a compiled kernel (issue #1092, 2026-11-08)

A compiled `kernelimage` run that left pixels to the interpreter used to say
only "N pixels: a branch that did not compile, or a certain error". With
fractal_clouds' `tropical_island` (size cap raised by hand, #1091), all 16,000
pixels retired and no soft-branch note appeared, so there was no way to tell
why.

### What was built

- **A reason per retired lane.** `Machine::kill` takes a reason code and
  counts the lanes it retires under it (`Machine::retired`). The codes are:
  - an `exec::Why`, for instructions whose reason is fixed by their kind and
    operator: integer overflow (the interpreter makes a real), idiv/mod by
    zero, inexact int→real into a mixed slot, the 2^53 check, `cvi` range,
    div by zero, a NaN compare, `0 0 atan`, `exp`, `sqrt`, `ln`, `log`, a
    real where an integer is needed, table index and null row, local array
    index and null element, a noise operand;
  - or a **site**, for `Abort` and `Assert`, which now carry one
    (`Kernel::sites`). A site's text says what it stands for, the operator
    being compiled, and the innermost three named procedures: "/x read where
    this pixel has not defined it, in f", "a real where an integer is needed
    (bitshift), in g", "a branch that did not compile (..., in a > b > c)".
    Texts are interned, so a site repeated in a loop or shared body is stored
    once.
- **The report.** The default note is one line, once per commonest reason:
  "N pixels retired: M for <reason>, K for R other reasons
  (PSCAT_KERNEL_TIMING=1 lists them); those run in the interpreter". When a
  soft branch or `srand` makes the whole image rerun, the note says so and
  why. `PSCAT_KERNEL_TIMING` adds a line per reason with its count.
- **Hot loop.** The counting is in a `#[cold]` function behind the existing
  `lanes == 0` test, and the reason is a `u32` instead of a `&str`, so a
  batch that retires nothing does the same work as before.

### Found with it: a lost soft branch (correctness)

`compile_loop` truncated `soft` when a loop pass was redone. A named body
shared (#1064) during the discarded pass stays in `defs`, and the redone pass
calls it, `Abort` and all, but its soft entry was gone. The kernel then had an
uncompiled branch that the driver did not know about: it took the
per-pixel fallback where only the whole-image fallback is exact (the branch
may `put` or `store`). `tests/kernels.rs`
(`a_shared_body_reused_after_a_redone_loop_pass_keeps_its_uncompiled_branch`)
gave different bytes from `/interpret` before the fix. The soft list is now
read off the finished kernel: the soft sites whose `Abort` is still in its
code, after `prune_subs` and DCE (`Compiler::soft_in`).

That is why `tropical_island` showed no soft-branch note.

### Arrays of booleans are tables

A one-level array of booleans with a run-time index (fractal_clouds'
`FsSpPalm FcSp get`, `FsSpBlob`, `FsSpAutOn`) was refused ("mixes numbers and
other values"), as a soft branch in `fc-tree-sp`. It is now a table of 0 and
1 (`Table::bools`) whose gather is a boolean register, exactly what a
compare leaves. Mixed booleans and numbers, and rows of booleans, stay
refused. On the GPU the table uploads as f32 0/1 like any other.

### tropical_island

With the cap at 1,000,000 its 16,000 pixels retired, in turn, for:

1. `fc-tree-species` writes the outside scratch array `FsMixWt` (`put`,
   soft). Inherent: the array's contents after the call are observable.
   fractal_clouds should use a per-call `/FcsW FsNsp array def` instead.
2. `fc-tree-sp`'s boolean columns (above): fixed here.
3. `fc-tree-shade-trunk`: `FcTKind 1 eq { FsSnagCol } { FsSpBark FcTSp get }
   ifelse` leaves an outside array on one branch and a table row on the
   other ("a stack entry differs between the branches of an if"; 185
   pixels). fractal_clouds should `aload` both and scale the three numbers.
4. Then the kernel is about 228k instructions: past the 1M emit cap while
   compiling, inside `fc-landscape-surface` (#1091's business). With the cap
   at 4M nothing retires, and the compiled render is byte-identical to
   `/interpret`.

The three cmp checks pass: the patched kernel render equals the patched
`/interpret` render, which equals the original `/interpret` render, which
equals the original (fully fallen-back) kernel render.

### pnw_coast

All 5,596 retired pixels are item 1 (`FsMixWt`), a soft branch, so the whole
image ran in the interpreter. With items 1 and 3 changed in fractal_clouds
(and the cap raised to 4M), none retire; the kernel is 251k instructions with
235 shared subroutines, and the render is byte-identical to `/interpret`
before and after the fractal_clouds change.

### Open

- With the fractal_clouds changes both scenes compile, but the compile takes
  110–130 s (sharing, 228–251k instructions): #1091's size cap and compile
  time are what is left.
