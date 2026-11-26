## `=only` and `==only` (issue #1413, 2026-11-25)

Ghostscript extensions (not in the PLRM) that print like `=` and `==` without the trailing
newline: `(x = ) =only x ==` is a common debugging idiom, and found code that uses it died with
`undefined`. Registered next to `=` and `==` in `src/ops/misc.rs`; all four call one `print_top`
(render, optional newline, `write_stdout`), so `=only` cannot drift from `=` nor `==only` from `==`
(including `==`'s output bounds, `OPERATOR_LIMITS`).

Semantics, read off Ghostscript 10.07.1 (`gs -q -dNODISPLAY -dBATCH -dNOPAUSE`):

- `=only` prints what `=` prints (strings as their bytes, names and numbers as text) and `==only`
  what `==` prints (PostScript syntax: `(a\(b\)\n)`, `/nm`, `[1 /a (s) {x y}]`, `-dict-`,
  `-mark-`), each with no newline. Checked on integer, real, boolean, null, name (literal and
  executable), string (literal, executable, empty, escapes), array, procedure, dict, mark and
  operator values; every one printed exactly `=`/`==`'s text minus the `\n`.
- Any object is accepted, one operand is consumed, nothing else is touched. Empty stack:
  `stackunderflow`, before any output, so a stopped handler sees `count` 0.
- Output order with `print` and an open `(%stdout) (w) file` is the order of the calls
  (`(A) print o (B) writestring (C) =only ... ` gives `ABCD12E`): same stream, as in gs.
- `systemdict /=only known` is true in gs and here; both are operators (`/=only load type`).

Differences kept on purpose, all inherited from `=`/`==` (the shared printer), not from the new
operators: `=` of an operator prints `--nostringval--` here and the name in gs; `=` of a string with
bytes above 127 is lossy here; large reals print as `1e20`/`123456789.5` here and `1e+20`/
`1.23457e+08` (`=`) / `123456792.0` (`==`) in gs. Also, gs implements `==only` as a procedure, so
its underflow reports `OffendingCommand: exch`; here it is `==only`. `/==only load type` is
`operatortype` here, `packedarraytype` in gs.

Not in the capability catalogue: like `min`/`max`, it is a Ghostscript-compat operator, not a pscat
extension (docs/dev/CAPABILITIES.md). The lint has no static operator list (it reads `systemdict`),
and no operator-count test exists, so nothing else needed updating. Tests: `tests/print_only.rs`
(shared cases run through pscat and, if installed, gs, byte for byte).
