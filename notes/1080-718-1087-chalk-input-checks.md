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
