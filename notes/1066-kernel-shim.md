## lib/kernel.ps: a `kernelimage` shim for other interpreters (issue #1066, 2026-11-06)

`kernelimage` and `kernelcompile` are pscat extensions (#742). Each of the
four "Computed pixels" gallery pieces carried its own ten-line `colorimage`
fallback so that Ghostscript could run it. `lib/kernel.ps` is that fallback,
shared:

```postscript
/kernelimage where { pop } { (lib/kernel.ps) run } ifelse
```

### What was built

- **`lib/kernel.ps`** defines `kernelimage` and `kernelcompile` only where
  `where` cannot find them, so under pscat the native operators win and
  loading the file defines neither name.
- **The shim's `kernelimage`** follows the interpreter path in
  `src/kernel/mod.rs` (`DRIVER`):
  - Each pixel runs as `i mark i j proc` inside `8 dict begin ... end`, the
    same operand layout as native.
  - Its state (sizes, matrix, procedure, row buffer, row counter) is a
    dictionary that is never put on the dictionary stack. The data
    procedure is built per call from two bound templates, with
    `kernel--subst` replacing the executable name `KS` by that dictionary
    and `KB` by the per-pixel body. A pixel that reads `/Row`, `/N` or
    `/KS` gets the caller's definition, or `undefined`.
- **Quantization** is pscat's `quantize`. NaN becomes 0, then the value is
  clamped to [0, 1], then `floor(c * 255 + 0.5)`. The NaN test (`dup dup ne`)
  must come first: under pscat, `NaN 0 lt` is an `undefinedresult`, and pscat
  can make a NaN (`1e308 10 mul dup sub`).
- **Errors** match native:
  - ncomp other than 1, 3 or 4, or a negative size: rangecheck.
  - A non-integer size or ncomp, a non-executable procedure, or a non-array
    matrix: typecheck.
  - The wrong number of components: rangecheck.
  - A component that is not a number: typecheck.
  - A zero width or height paints nothing.
- **`kernelcompile`** pops its operands and answers `false` and a reason
  naming `lib/kernel.ps`.

The helpers (`kernel--q`, `kernel--store`, `kernel--subst`, `kernel--image`,
`kernel--compile`) are top-level `@internal` definitions, in every
interpreter. That is what lets the `%%SelfTest` blocks test the shim under
pscat, where `kernelimage` is the native operator. The price is five
`kernel--` names in userdict, which a pixel could see but would not use by
accident.

### Tests

- **`%%SelfTest` blocks** (in `selftest/libraries.txt`) check:
  - quantization, including infinity and NaN;
  - the operand checks and component counts;
  - that a pixel sees a fresh dict and the caller's names, never the
    shim's;
  - that no pixel `def` survives the call;
  - that `i` is the column and `j` the row.

  Mutating the shim to drop the per-pixel `8 dict begin` or the NaN test
  turns them red.
- **`tests/kernel_shim.rs`** loads the shim under pscat with `where`
  answering false for the two names. Loaded that way, the shim defines both
  in userdict, where they shadow systemdict's. It then renders each case
  natively and through the shim and asserts the RGBA bytes are identical:
  - the guide's Mandelbrot;
  - gray clamping, CMYK with integer components, rounding boundaries,
    infinity and NaN;
  - caller definitions of every name the shim uses;
  - per-pixel defs;
  - the error cases.

  It also checks that a plain load under pscat leaves both operators in
  place.
- **Ghostscript** (gs 10.07, `-sDEVICE=png16m -r72 -g64x64`, run by
  `tests/kernel_shim.rs` when `gs` is installed) against pscat native:
  - a smooth gradient-and-sine kernel: max per-channel difference 1, with
    55 of 4096 pixels differing, mean 0.0134. These are 32-bit rounding at
    the `floor` boundary;
  - the 64x64 Mandelbrot: max 3, with 6 of 4096 pixels differing, mean
    0.0039. gs's 32-bit reals move an escape count by one at a few boundary
    pixels, and one count is 2.55 levels.
- **`tests/libbind.rs`** now walks `lib/kernel.ps` too, in pscat and in gs.

### Gallery

The four kernel pieces now load the shim instead of carrying the inline
fallback. Under pscat the `run` never happens, so their renders are
unchanged. Each piece now lists the `kernel` library in the manifest, and
`playground` stays false.

### Deferred

- The shim does not check the matrix beyond its type. A singular matrix is
  left to `colorimage` (native raises `undefinedresult` before painting).
- No capability rows: `kernelimage` and `kernelcompile` are already
  catalogued as `operator` entries. A `procedure` row for the shim would
  advertise a definition that never loads under pscat.
