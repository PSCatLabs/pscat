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

## #1080: every problem in one pass

`pscat chalk` stopped at the first unsupported character or command, so a board with several
took one retry each, and the message gave no position (and named `\"` inside `\text` as
`\inside-text`). Now:

- mathkit has `mkproblems` (`[lines] opts mkproblems -> [[line start end kind token] ...]`):
  a parse-only entry. While it runs a flag (`mkCollect`) makes the lexical errors (unsupported
  character, unsupported command, a bare operator word, an unsupported delimiter, a command
  inside `\text`) record themselves with their byte range and step over, so the ones after
  them are found too. A structural error (a missing brace) stops that line and is recorded;
  a "missing argument" that follows a stepped-over command on the same line is dropped as its
  echo. `mkpath`, `mkmetrics` and `mkalign` never set the flag: they are fail-fast as before.
- `\"` inside `\text{}` is named as written (`\"`, `\textbf`), in the fail-fast path too.
- The driver runs the check first and prints a `CKPROBLEM` line per problem. The first one
  then stops the program with the same `mk-<kind>:<token>` error as before, so a caller that
  only reads that (`chalk::explain`, the hosted server) gets the first problem, unchanged.
  `Driver::explain(stdout, err)` returns the whole list as `ChalkError::Problems`.
- Positions are the input's: `chalk::layout` carries, for every byte of every row, the offset
  it came from, through comment stripping, whitespace normalisation, label removal, delimiter
  removal, wrapper stripping, row splitting and row-break option removal. Lines count from 1,
  columns count characters. The delimiter errors of #718 carry places too.
- Output: one problem is one line (`... at 1:3`); several are a list, at most 20 shown. The MCP
  tool returns `problems` (`code`, `token`, `line`, `column`, `offset`, `message`); the
  top-level `code` and `token` are the first one's. At most 200 are collected.
- Hints: a few German transliterations (`ö` -> `oe`), "write the plain letter" for other
  accented Latin letters and accent commands, and TeX's opening quote. `pscat chalk --help` lists
  the characters `\text{}` sets; a test checks that list against mathkit.
- Library API: `chalk::ChalkError` gains a variant (`Problems`), `chalk::Problem` and
  `Driver::problems/explain` are new; `Driver` gains a private field. pscat-mcp (#1098) matches
  on `ChalkError`: it needs the new arm, and to call `Driver::explain` with the child's stdout
  to give the list (`explain` alone still gives the first problem).

Not done: the structural errors stop their line, so a row with a missing brace hides later
problems in that row until the brace is fixed; mathkit's errors raised during layout (rare,
after parsing) are still one at a time.
