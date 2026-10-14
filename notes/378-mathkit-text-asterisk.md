# #378: ASCII * inside \text{}, \mathrm{} and \operatorname{}

#364 made ASCII `*` in math mean `\ast` (`mkascii` entry 42) and left it
refused in text runs, which take glyph names straight from strokeglyphs'
`sgmetrics`: ASCII `*` had no glyph there (`mk-unsupported-character:*`).

- Decision: add a real text glyph rather than map to `/asteriskmath` in text
  runs. TeX text mode sets a raised asterisk, and the math one is a centred
  binary-operator glyph; two names keep both right.
- strokeglyphs: new `/asterisk` skeleton (advance 320, three strokes through
  a centre at y=585, arms 105 long: same construction as `/asteriskmath`,
  raised to the top of the x-height) and `42 /asterisk` in `sgcmap`. No existing glyph
  changed. In sgpath text a `*` used to be a missing glyph (an error, or a
  silent 300-unit gap under `/Strict false`), so only text that already failed
  or relied on the gap can render differently.
- mathkit: no code change. Math-mode `*` takes the `mkascii` branch before
  any strokeglyphs lookup, so `a*b` and `z^*` are untouched. Text, mathrm and
  operatorname runs reach `/asterisk` through `sgmetrics`.
- Tests: the selftest line and Rust case that pinned `\text{a*b}` as refused
  are gone. New: `ascii-star-in-text-is-a-raised-asterisk` (mathkit),
  `ascii-star-is-the-text-asterisk` (strokeglyphs), and Rust
  `a_star_in_text_runs_is_a_raised_asterisk`.
- Identity: `scripts/identity_check.sh --loader lib/strokeglyphs.ps --loader
  lib/mathkit.ps` byte-identical, with a control that must differ.
- Chalkboard: gallery/chalkboard_lecture.ps from PR #353's branch (it has no
  `lib/` changes of its own, so the merge-base tree is its base) rendered
  byte-identically (`cmp`) from the base tree and this branch, at 144 dpi, each
  with its own tree as the working directory. Its own control, a `/Strict false`
  `(a*b)` sgpath drawn through the same two commands, differs between the
  trees, so the cwd really picked each tree's libs.
- Demo surface: none added. A new specimen or gallery entry would change a
  committed render; the glyph's look is shown in the PR instead.
