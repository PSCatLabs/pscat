# Compiled per-pixel kernels: `kernelimage` and `kernelarray`

Some pictures are computed one pixel at a time: ray tracers, fractals,
noise landscapes, image filters, a small neural network run at every
pixel. In plain PostScript the per-pixel procedure is interpreted token
by token, and each token costs 5–20 ns whatever it does. A ray tracer at
1440×960 then takes minutes.

`kernelimage` takes the same per-pixel procedure and **compiles** it.
Stack shuffles, name lookups and procedure calls disappear at compile time.
What is left (the arithmetic, the loops and the tests) runs on every core,
16 pixels at a time, and the result is **byte for byte** what the interpreter
makes of it.

On a 10-core Apple M4 laptop, release build:

| picture | interpreter | `kernelimage` | |
|---|---:|---:|---:|
| [Mirror Garden](../../gallery/mirror_garden.ps): ray tracer, 1440×960, 4 rays a pixel | 149 s | 0.9 s | 165× |
| a Mandelbrot zoom, 480×320 | 4.8 s | 0.06 s | 75× |
| the fractal_clouds landscape renderer, one 160×100 frame | 18 s | 0.29 s | 61× |

More measurements are in [`notes/742-compiled-kernels.md`](../../notes/742-compiled-kernels.md).

## The operator

```
width height matrix proc ncomp kernelimage -
```

- `width height` give the sample grid, `matrix` is the image matrix and
  `ncomp` is 1 (gray), 3 (RGB) or 4 (CMYK), exactly as for `colorimage`.
- `proc` is called as **`i j proc c1 … cn`** for every sample. `i` is the
  column (0 at the left) and `j` the row (0 at the *first* row of image data),
  both integers. Each component is a number from 0 to 1; values outside are
  clamped, and each becomes a byte as `floor(c × 255 + 0.5)`.
- The result is painted like an 8-bit `colorimage`, so clipping, PNG, SVG,
  PDF and `--bands` all work as they do for images.

```postscript
/W 480 def /H 320 def
/pixel {                          % i j -> r g b
  /j exch def /i exch def
  /cr i W 2 div sub W div 3 mul 0.7 sub def
  /ci H 2 div j sub W div 3 mul def
  /zr 0.0 def /zi 0.0 def /n 0 def
  { zr zr mul zi zi mul add 4 gt n 100 ge or { exit } if
    zr zr mul zi zi mul sub cr add  zr zi mul 2 mul ci add
    /zi exch def /zr exch def /n n 1 add def } loop
  n 100 div dup dup 0.5 mul exch 1 exch sub      % r g b from the count
} bind def
gsave 480 320 scale
W H [W 0 0 H neg 0 H] /pixel load 3 kernelimage
grestore showpage
```

`kernelimage` is a pscat extension. A program meant for other interpreters
too, Ghostscript for instance, can load the shim
[`lib/kernel.ps`](../../lib/kernel.ps) when the operator is missing:

```postscript
/kernelimage where { pop } { (lib/kernel.ps) run } ifelse
```

The shim defines `kernelimage` and `kernelcompile` (and `kernelarray` and
`kernelarrays`, below) only when they are not defined already, so under pscat the native operators win. Its `kernelimage`
runs the same procedure through `colorimage`, one fresh dict per pixel and
quantized as above, so the picture matches pscat's up to the other
interpreter's arithmetic (Ghostscript's reals are 32-bit). Its
`kernelcompile` always answers `false`. A program that loads it reads a
file, so it cannot run in the browser playground.

Ghostscript's default `-dSAFER` mode refuses that `run`
(`invalidfileaccess`). From the repo root, either allow the read or load the
shim first on the command line:

```sh
gs --permit-file-read=lib/ -sDEVICE=png16m -o out.png gallery/dreaming_network.ps
gs -sDEVICE=png16m -o out.png lib/kernel.ps gallery/dreaming_network.ps
```

### Every pixel gets a fresh dictionary

Each pixel runs as if by `8 dict begin i j proc end`:

- `def` writes the pixel's own dictionary.
- A name the pixel has not defined yet is found in the dictionaries as they
  were when `kernelimage` was called.
- Nothing one pixel defines is seen by the next, or after the call.

So pixels cannot pass state to one another. This is what lets them run in any
order on any core. A "carry the previous row" trick, like keeping the last
height for shading, has to become a pure function of `i` and `j`: compute the
neighbour's height again.

A one-time setup done lazily inside the procedure (`Ready not { setup /Ready
true def } if`) runs on *every* pixel. Do it once before calling
`kernelimage`.

## Grids of numbers: `kernelarray`

```
w h proc n kernelarray array
w h proc n kernelarrays a1 … an
```

Many programs compute a grid of numbers before they draw: a height map
sampled from a terrain function, a slope or density map built from it, a
lookup table. `kernelarray` compiles that per-cell procedure as
`kernelimage` does and returns the values as numbers instead of painting
them.

- `proc` is called as **`i j proc v1 … vn`** for every cell of a `w` by `h`
  grid, `n` from 1 to 64, with the same fresh dictionary per cell.
- `kernelarray` returns a new array of `w × h × n` numbers, row-major with
  the `n` values of a cell together: cell `(i, j)`'s values start at
  `(j × w + i) × n`.
- `kernelarrays` returns them as `n` arrays of `w × h` instead, `a1` holding
  every cell's `v1` (at `j × w + i`). Use it when a pass makes several maps
  at once.
- Each value is exactly what the interpreter leaves: the same integer or
  real, bit for bit, -0.0 included. The one exception is NaN: any NaN is
  stored as the quiet NaN `0x7ff8000000000000`, in both modes. A NaN's sign
  and payload depend on the CPU and the compiler, and nothing else in
  PostScript can tell NaNs apart.
- The results are ordinary new arrays, writable, and a later pass can read
  them (as a table, so that compiles too).
- `w × h × n` may be at most 16,777,216 (`array`'s own limit); past that it
  is a `limitcheck`.

```postscript
/N 512 def
/Height N N { /j exch def /i exch def        % one height per cell
  i 0.01 mul j 0.01 mul 6 0.5 2 fbm2 } 1 kernelarray def
/Slope N N { /j exch def /i exch def         % a second pass reads the first
  /c j N mul i add def
  Height c get  Height c i N 1 sub lt { 1 } { 0 } ifelse add get  sub abs
} 1 kernelarray def
```

The reference, under `/interpret .kernelmode`, runs the procedure for each
cell in raster order (row 0 first), each as if by `8 dict begin mark i j proc
end`, and takes the `n` values above the mark: a wrong count is a
`rangecheck`, a value that is not a number a `typecheck`, and a cell that
leaves with `exit` an `invalidexit`. That is the same as a nested `for` loop
that `put`s each value whenever the procedure is pure, as a compiled one is.
Everything below about what compiles, how it degrades, `.kernelmode` and the
limits applies to `kernelarray` too. `kernelcompile` answers for it as well
(it compiles for `kernelimage`; the kernel for `kernelarray` differs only in
how it keeps its outputs).
`--gpu-preview` does not apply: its values would not be exact. Under
`--bands` every band computes the whole grid, since the array is the result.
There is no form that fills an existing array: a procedure that read the
array it fills would see it change under the interpreter but not compiled.

The shim [`lib/kernel.ps`](../../lib/kernel.ps) defines `kernelarray` and
`kernelarrays` too, the interpreted way.

## What compiles

The compiler handles ordinary numeric PostScript, the kind per-pixel code is
made of:

| | |
|---|---|
| numbers, booleans, `true false null` | |
| `dup exch pop copy index roll mark counttomark cleartomark count` | free: resolved at compile time |
| `def`, `load`, `where`, `known`, `N dict begin … end`, `bind` | names become registers |
| `add sub mul div idiv mod neg abs ceiling floor round truncate cvi cvr sqrt exp ln log sin cos atan min max` | the interpreter's exact arithmetic: integers stay exact to 64 bits, as they do in the interpreter |
| `eq ne lt le gt ge and or xor not bitshift` | |
| `if ifelse for repeat loop exit forall exec` | short constant loops are unrolled |
| procedures, by name or inline | inlined |
| `[ … ]`, `aload astore length get put`, `N array` | arrays made in the kernel |
| `get` on an array or string defined outside | a table read: colour ramps, weights, an image's rows |
| `gnoise2 gnoise3 vnoise2 vnoise3 fbm2 fbm3 vfbm2 vfbm3 ridged2 ridged3 gnoise2d` | the native noise operators |
| `srand rand rrand` | when the pixel seeds its own stream first (`i j hash srand`): see below |

Data from outside the kernel (arrays, strings, an image decoded into an
array of row strings) is **read-only** inside it.

What does not compile:

- painting, text and I/O;
- `rand` before the pixel's own `srand` (it would read the state the
  previous pixel left), and `store`;
- `put` into an array from outside;
- strings and dictionaries as values;
- recursion.

### Random numbers

A pixel that seeds its stream from its position, then draws, is a pure
function of `i` and `j`, so it compiles:

```postscript
/pixel { /j exch def /i exch def
  i 16#45D9F3B mul j 16#2545F491 mul xor 16#7fffffff and srand
  rand 255 and 255 div  rand 255 and 255 div  rand 255 and 255 div } bind def
```

- `--sweep-seed` forces every pixel's `srand` as it does in the
  interpreter, and an `rrand` value given back to `srand` is still a
  restore.
- After the call, the random state is the one the last pixel (in raster
  order) that called `srand` left, as in the interpreter, `--bands` included.
- A `rand` that some pixel reaches before its `srand` is not compiled. When
  no pixel can seed first, the whole procedure runs in the interpreter.
  When only some pixels skip the `srand`, and whenever a seeding kernel has
  a pixel to leave to the interpreter, the whole image runs there.

How it degrades:

- **The whole procedure does not compile:** `kernelimage` runs it in the
  interpreter, with a one-line note on stderr saying why and in which
  procedures.
- **One branch does not compile:** the pixels that take it are interpreted,
  one by one after the compiled ones, when nothing the procedure can reach
  changes what another pixel reads. That rules out writing into anything the
  pixel did not make (`put`, `putinterval`, `astore`, `copy`, `store`,
  `begin`), `srand` and `rand`, files and graphics, and turning data into
  code (`cvx`, `load`). Any of those anywhere in reach, even in code the pixels never
  run, and a pixel that takes such a branch sends the whole image to the
  interpreter. The stderr note names what was found and where (the first
  four; `PSCAT_KERNEL_TIMING=1` lists all), and `kernelcompile` says which
  of the two it will be.
- **A pixel would raise an error** (a division by zero, a table index out of
  range) **or overflow a 64-bit integer:** only that pixel is interpreted. It
  then raises the real error, at the same pixel, or makes the real result.

So the picture is always exactly the interpreter's. Only the speed changes.

## Checking and tuning

- `proc ncomp kernelcompile bool string` says whether `proc` compiles, and if
  not, why: `{ … } 3 kernelcompile`.
- `/strict .kernelmode` makes an uncompiled procedure (or branch) an error
  that names the reason. `/interpret .kernelmode` always uses the
  interpreter, the reference to compare a render against. `/auto
  .kernelmode` is the default.
- A very large procedure (past 400,000 instructions as compiled, all calls
  inlined) is compiled again with the bodies it calls often shared as
  subroutines, with a cap of 2,000,000 instructions as compiled. The count is
  of code before dead-code removal, so the kernel kept is usually far
  smaller. `/large .kernelshare` asks for that sharing always, and
  `/never .kernelshare` turns it off. `kernelcompile` reports the
  subroutine count.
- `PSCAT_KERNEL_TIMING=1` prints the compile and run times, the kernel's
  size, and the instruction set and math mode in use. `PSCAT_KERNEL_THREADS=N` sets the number of worker threads (the
  default is every core; one under `--bands`).

### Faster `sin`, `cos` and `atan`: `.kernelmath`

```postscript
/exact .kernelmath     % or /fast, the default
```

By default a compiled kernel computes `sin`, `cos` and `atan` on a whole
batch of pixels at once, with vector instructions (`/fast`). `/exact
.kernelmath` computes them one pixel at a time through pscat's own math
library, as the interpreter does. The environment variable
`PSCAT_KERNEL_MATH=exact` starts every program in `/exact`; a program's own
`.kernelmath` still wins.

- **The results are the same bits.** The batch code is the same library's
  algorithms, written for vectors. It is checked bit for bit against the
  one-at-a-time code over millions of inputs, and every picture tested
  matched. `/exact` is there to rule it out if a picture ever differs.
- **Speed.** On an Apple M4, per pixel, `sin` takes 5.9 ns instead of 9.5,
  and `atan` 6.1 instead of 13.7. The Mandelbulb took about 20% fewer
  cycles. A kernel with little trigonometry gains nothing, and x86 CPUs
  with AVX2 should gain more, since their vectors hold four numbers.
- **Where it applies.** Only `kernelimage`. `kernelarray` always uses
  `/exact`. `exp`, `ln` and `log` are unchanged.
- **Unusual angles.** Infinite and NaN angles, angles past about 94 million
  degrees, and `atan` with a zero, an infinite operand or a denominator of
  1.0 use the one-at-a-time code. So `0 0 atan` is still the error it is.

### Instruction sets

On x86_64, compiled kernels use the best vector instructions the CPU has:
the baseline SSE2, SSE4.1 (x86-64-v2) or AVX2 (x86-64-v3). The pictures are
the same in each, since fused multiply-add is never used.
`PSCAT_KERNEL_TIMING=1` names the one in use, and `PSCAT_KERNEL_ISA=base`
(or `v2`, `v3`) asks for a lower one. Other CPUs, Apple silicon among them,
have one set.

Tips:

- Write the per-pixel procedure as a pure function of `i` and `j`, and keep
  setup (tables, weights, camera vectors) outside it.
- Avoid naming variables after operators (`fill`, `ln`, `sin`). `bind` turns
  such a name into the operator.

## Limits

`--max-time` and `--max-steps` apply inside a kernel. Workers look at the
clock between batches and every 256 loop trips, so a runaway `{ } loop`
stops with a time error. The image buffer and the table snapshots count
against `--max-memory`.

**Known limitation under `--bands`.** Each band computes only the image rows
that reach it, so an error that only a row outside the band would raise is
not raised in that band. A plain render, and a render with `/interpret
.kernelmode`, raise it in every band. The stitched picture differs only for
a program whose per-pixel procedure errors *and* that catches the error
(`stopped`) and draws differently afterwards. Without `stopped`, the render
fails either way.

## GPU preview

A pscat built with the `gpu` cargo feature (`cargo build --release
--features gpu`) has `--gpu-preview`.

- Compiled kernels are lowered to WGSL and run on the GPU through wgpu: Metal
  on macOS, Vulkan or DX12 elsewhere.
- It computes in 32-bit floats and does not check PostScript errors, so the
  picture is close to the CPU's but not the same bytes. Integer hashes in
  particular lose precision.
- It is meant for previews: on an M4 the gallery kernels run 23–68× faster
  than on its ten CPU cores.
- Final renders stay on the exact CPU path.
- On the GPU, one budget caps every loop trip of a pixel at 2^20, nested
  loops included, so a runaway kernel still finishes. It finishes with an
  image, not an error: a loop the CPU would stop with a time error gives a
  capped, often blank picture.
- It runs under `--max-time` and `--max-steps` too (#1065). A GPU dispatch
  cannot be interrupted, so the limits are checked before each dispatch of
  65536 pixels, and a limit stops the image with the same error as on the
  CPU. A dispatch is charged the kernel's straight-line size per pixel in
  steps; GPU loop trips are capped, not counted. One dispatch can run past
  the time limit; pscat-mcp's wall-clock kill covers that.
- pscat-mcp's `render_postscript` and `start_render` take it as
  `gpu_preview: true` ([agents guide](agents.md#gpu-preview)).
