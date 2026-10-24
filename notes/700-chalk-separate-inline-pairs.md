# chalk: separate inline math pairs (#700)

`pscat chalk '\(a\) + \(b\)'` used to fail with `unsupported LaTeX command \)`:
`strip_delims` removed the outer `\(` and `\)` of the whole input as if they
were one pair, and mathkit got the stray inner ones. `\[a\] \[b\]` failed
the same way. `$a$ + $b$` did not fail, but only because mathkit ignores a bare
`$`, so the board was drawn from `a$ + $b`.

**What changed** (`src/chalk.rs`, `unwrap_math` replaces `strip_delims`)
- A scan over the input, pairing each backslash with what follows (so `\\(`,
  `\$` and `\right)` are not delimiters), tracks the open pairs on a stack.
  Every delimiter of the four forms (`$$`, `$`, `\[ \]`, `\( \)`) becomes a
  space, so several pairs, side by side or nested, make one formula
  (`\(\alpha\)\(b\)` is `\alpha b`, not `\alphab`). mathkit ignores spaces.
- The scan starts only when the input begins with `$` or holds a backslash
  delimiter. A bare `$` in the middle of plain input still reaches mathkit
  unchanged, as before.
- Errors are `ChalkError::Latex` (no new variant, so the MCP tool and CLI
  need nothing): `unclosed_math_delimiter`, `unmatched_math_delimiter`
  (a closer with no opener, or not the innermost pair's), and
  `text_outside_math` (a letter outside every pair: TeX sets that as text,
  which a board does not). Control words outside a pair (`\quad`) are fine.

**Decision: join, not reject.** The issue allowed either. The issue's own
example has operators between the pairs, and joining gives the board the user
meant. Prose between pairs is rejected rather than set as italic math.

**Evidence.** The golden hashes in `tests/chalk.rs` pass unchanged. A
`--print-ps` comparison of a base binary (origin/main) and the head binary over
18 inputs (every wrapper, rows, matrices, comments, `\%`, a bare `$`) is
byte-identical. `scripts/identity_check.sh` does not apply: its corpus is `.ps`
files, which never reach `chalk::lines`.
