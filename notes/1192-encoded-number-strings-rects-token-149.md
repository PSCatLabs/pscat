## Encoded number strings in the rect operators, and binary token 149 (issue #1192, 2026-11-12)

Follow-up to #1180 (`src/numstring.rs`, `notes/1180-xshow-yshow-xyshow-cshow.md`).

**Rect operators.** `rectfill`, `rectstroke` and `rectclip` (`pop_rects` in
`src/ops/graphics.rs`) take `x y w h`, or a list of numbers as an array or
as an encoded number string (PLRM 3.14.5) through `NumOperand`. The checks
come in Ghostscript 10.07.1's order (`tests/numarray.rs` pins the report):

1. the operand's type (`typecheck`; a plain string is a `typecheck` too, via
   the header check),
2. a string's header: shorter than 4 bytes or first byte not 149 is
   `typecheck`; an undefined representation or a count that does not match
   the bytes is `rangecheck`,
3. a count that is not a multiple of 4: `typecheck` (arrays and strings
   alike; it comes before the values, so a non-finite real in a 3- or
   5-number string is a `typecheck`, not `undefinedresult`),
4. the values: a non-number array element is `typecheck`, a non-finite
   encoded real `undefinedresult`.

A 6-element array in the top position of `rectstroke` is still always the
matrix, never a rect list; an encoded string there is a rect list, and
may have a matrix on top of it. One stray byte after the last value is
allowed.

**Token 149** (PLRM 3.14.2, homogeneous number array). The lexer frames
`149 repr count(2) numbers...` as `Token::NumberArray`, a list of
`Integer`/`Real` tokens, and it scans to a *literal* array (`token`, a
program, a procedure body, `cvx exec` all alike). It reuses the
representation decoding of `numstring` (`token_len`, `token_numbers`). As in
gs it differs from the string operand in that:

- a truncated token, a short header or an undefined representation is a
  `syntaxerror` (not typecheck/rangecheck);
- the token ends after exactly `count` numbers: a stray byte or whole extra
  value stays in the input;
- a fixed-point representation with no fraction bits (0, 32, 128, 160) gives
  integers, any other (including 16- and 32-bit with fraction bits, and
  reals) gives reals;
- a non-finite real is an `undefinedresult`;
- it is switched on and off with `setobjectformat` like 128 to 131 (with
  `0 setobjectformat`, byte 149 is an ordinary name character).

Pinned from gs in `tests/numarray.rs` (`TOKEN_EXPECTED`).

**Not done / seen on the way.**

- Binary tokens 132 to 148 and 150 to 159 are still not scanned.
- Two differences from gs that the tests steer around, not caused by this
  change: `rectstroke` with a non-uniform matrix (`[2 0 0 0.5 0 0]`) draws
  vertical sides 2 px wide where gs draws them about 4.5 px wide; and after
  an emptying `rectclip`, `clippath pathbbox` is `nocurrentpoint` here where
  gs gives a degenerate path. The tests use a uniform matrix and no
  `clippath` on an empty clip.
