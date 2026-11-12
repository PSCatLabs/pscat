# #718, #1087, #1080: chalk input checks and the looped phi

Bundled in one PR (#1080, #718, #1087).

## #718: a stray `$`, prose between pairs, `\big` fences

Behaviour change (inputs that rendered before now fail; say so in the release notes):

- Any unescaped `$` in a chalk input is a math delimiter, not just a leading one.
  `a $ b` and `x = 5$` were drawn with the `$` dropped; they are now an
  `unclosed_math_delimiter` error ("math delimiter $ is never closed ..."), as LaTeX
  stops with "Missing $ inserted". `\$` is not a delimiter. `x = $y$` and `a $x$ b` also
  fail now: the words outside the pair are `text_outside_math`.
- Pairs are judged before the words around them, so `\(x\) where $` names the stray `$`
  rather than "where".
- Prose between pairs stays an error (LaTeX would set it as text, which a board does not;
  wrapping it in `\text{}` is not unambiguous). The message now names the whole word and
  shows the fix: put it in `\text{...}` inside one pair.
- `\big`, `\Big`, `\bigg`, `\Bigg` (and their `l`, `r`, `m` forms) are fences like `\left`
  and `\right`: `\bigl\(` is mathkit's "unsupported delimiter: \(", not an unclosed pair.
- The change is in `chalk::lines` (Rust). mathkit still ignores `$`, so a direct `mkpath`
  caller is unchanged.

## #1087: `\varphi` no longer reads as psi

`/phi.loopy` (mathkit's `\varphi`) was one slanted stroke with a small loop low on its left,
which at 22 to 30 pt looked like half a psi. It is now a closed loop with a near-vertical,
slightly leaning stem running through it (two strokes, advance 580), with the loop's end
overshooting into a small hook so it stays hand-drawn. A psi's cup is open at the top; this
loop is closed. `\phi` (the circle and bar) is untouched. The committed
`strokeglyphs_subscripts_greek_specimen.png` was already stale against `main` (a re-render of
the example at the merge-base differs from it everywhere), so it is not regenerated here.
