## mathkit: ASCII `*` and `^*` (issue #364, 2026-10-14)

Found while working #339: `\psi^*` failed with `mk-unsupported-character:*`.
Complex conjugates (`z^*`) and adjoints (`A^*`, `f_*`) are everyday board
math, and #339 had added `\ast` (glyph `/asteriskmath`) only.

### What changed (lib/mathkit.ps)

- `mkascii` gets `42 [/asteriskmath /bin]`: ASCII `*` is `\ast`, TeX's own
  reading (mathcode "2203, class bin, glyph cmsy 03). No new code path.
- No special raised-asterisk glyph. TeX has none either: `z^*` is the
  centred `/asteriskmath` set at script size and lifted by the superscript
  shift, and `A_*` the same lowered. The existing script and class rules do
  the rest: a `*` at the start of a formula or after another operator
  becomes an ordinary symbol (no medium space), `a*b` gets the binary-operator
  spaces, `z^*` and `z^{*}` are the same.
- New selftest `ascii-star-is-ast`: `a*b` = `a \ast b` and `z^*` =
  `z^{\ast}` in width, a leading or trailing `*` takes no space (widths just add), `z^*` rises above `z`,
  `A_*` hangs below the baseline, and a set of starred scripts, fractions,
  sums and `\left(...\right)^*` lay out and draw.
- The old guard `(a * b) -> mk-unsupported-character:*` moved to `@`
  (selftest and `tests/mathkit.rs`); the header and README list `*`.

### Not changed

- `*` inside `\text{}`, `\mathrm{}` and `\operatorname{}` is still
  `mk-unsupported-character:*`: those run through strokeglyphs' text names,
  where ASCII `*` has no glyph. A new selftest line pins it. TeX text mode
  would give a raised asterisk there; tracked as a follow-up issue.
- Committed renders: `scripts/identity_check.sh --loader lib/strokeglyphs.ps
  --loader lib/mathkit.ps` against the merge-base worktree, one release binary
  (the main checkout's) for both trees: 14 of 14 pages identical across 7
  files, and the control (a driver drawing `z^* = a*b`, which errors in the
  base) differs, as it must. `gallery/chalkboard_lecture.ps` from PR #353
  (branch feature/238-professors-chalkboard-lecture) renders byte-identically
  (`cmp`) in both trees; it contains no `*`, so that was expected.
- Follow-up: #378 (`*` inside `\text`).
