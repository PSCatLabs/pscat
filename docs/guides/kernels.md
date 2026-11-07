# Compiled per-pixel kernels: `kernelimage`

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
too can define a fallback when `/kernelimage where` fails. Each gallery piece
in [Computed pixels](../../gallery/README.md) carries a ten-line fallback that
runs the same procedure through `colorimage`.

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

Data from outside the kernel (arrays, strings, an image decoded into an
array of row strings) is **read-only** inside it.

What does not compile:

- painting, text and I/O;
- `rand`, `srand` and `store`;
- `put` into an array from outside;
- strings and dictionaries as values;
- recursion.

How it degrades:

- **The whole procedure does not compile:** `kernelimage` runs it in the
  interpreter, with a one-line note on stderr saying why and in which
  procedures.
- **One branch does not compile:** only the pixels that take it are
  interpreted.
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
- `PSCAT_KERNEL_TIMING=1` prints the compile and run times and the kernel's
  size. `PSCAT_KERNEL_THREADS=N` sets the number of worker threads (the
  default is every core; one under `--bands`).

Tips:

- Write the per-pixel procedure as a pure function of `i` and `j`, and keep
  setup (tables, weights, camera vectors) outside it.
- A name defined only inside a loop and read after it does not compile.
  Define it before the loop.
- Avoid naming variables after operators (`fill`, `ln`, `sin`). `bind` turns
  such a name into the operator.

## Limits

`--max-time` and `--max-steps` apply inside a kernel. Workers look at the
clock between batches and every 256 loop trips, so a runaway `{ } loop`
stops with a time error. The image buffer and the table snapshots count
against `--max-memory`.

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
