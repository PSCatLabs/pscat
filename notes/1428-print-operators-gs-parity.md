## `=` and `==` against Ghostscript: operators, high bytes, exponents (issue #1428, 2026-11-26)

#1413 left three differences between the shared printer (`=`, `==`, `=only`, `==only`, and through
`Object::text`: `stack`, `cvs`, `$error /command`) and gs 10.07.1. Each was settled with a gs run
(`gs -q -dNODISPLAY -dBATCH -dNOPAUSE`); `tests/print_gs_parity.rs` pins the lines and, with gs
installed, runs one program through both interpreters byte for byte.

1. **Operator names: fixed.** gs: `/add load =` prints `add`, `stack` prints `add`, `cvs` gives
   `add`, `==` prints `--add--`. pscat printed `--nostringval--` from `=` and `stack`, and only `cvs`
   (a private `cvs_text`) gave the name. `Object::text` now gives the name and `cvs_text` is gone.
   `$error /command get =` prints `div`. A mark, null, array, procedure, dict print
   `--nostringval--` in both and are unchanged.
2. **Bytes above 127: fixed.** gs writes `(a\351b\377\200) =` raw (`a e9 b ff 80`). pscat decoded
   the string as UTF-8 and wrote U+FFFD. `=` now writes the string's storage as it is (no decode
   and no copy of a large string), `stack` cuts a string on bytes (`text_bytes_bounded`, same
   4096-byte cut), and `cvs` copies the bytes (`(a\351b) 10 string cvs ==` was `(a\357\277\275b)`).
   `cvs` refuses a string longer than the buffer with `rangecheck` before it copies anything.
3. **Large reals: the spelling only.** gs spells an exponent `1e+20`, `-1e+20`, `1e-05`; pscat
   (Rust's `{:?}`) gave `1e20`, `1e-5`. `format_real` now writes a sign and at least two digits.
   Left as it is, on purpose:
   - gs prints a *float32*. `123456789.5 =` is `1.23457e+08` (six significant digits, `%g`) and
     `123456789.5 ==` is `123456792.0` (the float32's own value); `123456.7 =` is `123457.0`;
     `0.1 0.2 add =` is `0.3`; `1 3 div ==` is `0.333333343`. gs's two printers disagree with each
     other, and pscat reals are f64 and print every digit (`0.30000000000000004`). Copying either
     would lose information.
   - gs switches to an exponent at `1e6` under `=` (`1e6 =` is `1e+06`, `1e15 =` is `1e+15`) and
     at a digit-dependent point under `==` (`1234567.0 ==` is `1234567.0`, `1e6 ==` is `1e+06`);
     pscat switches at `1e16` and below `1e-4` (Rust's rule), so `1e6 =` is `1000000.0` and
     `cvs` of a round number stays plain for a label. Changing it would change what `cvs` puts in
     a chart's text, for no gain; it is a follow-up if anyone needs gs's switch.
   - gs raises `undefinedresult` for `1e20 1e20 mul` (float32 overflow); pscat has f64 range.

The exponent spelling reaches every user of `format_real`: `==`, `pstack`, `cvs`, `repr()`, the MCP
and host API's printed text. Everything prints and reads back (the scanner accepts `1e+20`).

Resource use (Codex round 1): a string `cvs` makes a copy of up to its buffer's length, so it is
asked of the memory cap (`check_alloc`) before it is made and charged a step per KiB
(`poll_limits`), as a file read is; the too-small-buffer `rangecheck` still comes first, with no
copy. `=`, `=only` and `print` of a string write straight from its storage (no copy; `print` used
to `to_vec()` it, an allocation the cap never saw) and charge the same step per KiB. `stack` and
`pstack` are bounded by constants (4096 bytes a string, 32 KiB in all). `tests/cvs_limits.rs`
(its own test binary, as the memory limit counts the whole heap). With `--max-memory 24` a 16 MiB
string's copy is refused by the memory cap before the step cap is looked at; the step cost shows
with a looser memory cap.

Not changed: gs prints a negative zero as `0.0` everywhere (`-0.0 = == cvs pstack`, and `-1e-50 =`),
so `-0.0 3 string cvs` works there and is a `rangecheck` here (`-0.0` is four bytes). pscat keeps
the sign on purpose: `tests/dispatch_semantics.rs` pins `3 -0.0 mul` as `-0.0` and the kernel tests
tell the zeros apart, so dropping it is more than one line and a visible debugging loss.
