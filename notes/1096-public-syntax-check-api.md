## Public syntax-check API, `pscat::syntax::check_syntax` (issue #1096, 2026-11-11)

pscat-mcp's `validate_postscript` ran a syntax pass on the `#[doc(hidden)]`
`pscat::lexer`: count tokens, balance `{`/`}`, report the first lexer error.
That module is outside the documented API, so a lexer refactor could break it
unannounced.

**Built.** `src/syntax.rs`, a documented `pub mod syntax` above the hidden
block in `lib.rs`:
`check_syntax(&[u8]) -> Result<SyntaxSummary, SyntaxError>`. `SyntaxSummary
{ tokens }`; `SyntaxError { kind, message, offset, line, tokens }` with
`kind` one of `Lexical`, `UnmatchedCloseBrace`, `UnterminatedProcedure`. Both
structs and the enum are `#[non_exhaustive]`, so fields and kinds can be added
in a minor release. `pub mod lexer` is unchanged and still hidden.

**Decisions.**
- `Result` as the issue proposed, but `tokens` also sits on the error: the
  host's old `SyntaxScan` reported the count on failure, so a one-for-one
  swap needs it. `ok` is `is_ok()`, `error` is `message`.
- Messages keep pscat-mcp's wording (`unmatched '}' (token N)`,
  `unterminated procedure: N '{' without a matching '}'`, the lexer's own text
  for scanner errors), and tokens are counted the same way, so its tests
  survive the swap. `tests/syntax.rs` runs a corpus through a copy of the old
  loop and through the new function and compares ok, count and message.
- Positions are the start of the offending token, not where scanning stopped:
  the `(` of an unterminated string (the scanner is at EOF by then), the stray
  `}`, and for a missing `}` the *outermost* unclosed `{` (EOF says nothing).
  The offset comes from a new `Lexer::token_start_pos`, captured beside
  `token_start_line`. Lines need `Lexer::main_program`; `Lexer::new` reports
  none.
- Only `LBrace`/`RBrace` are matched, with a catch-all arm, so new `Token`
  variants (the binary object sequence of #733) do not break it. A token that
  starts with 128 to 131 becomes a binary object sequence once #733 lands; a
  truncated one is an ordinary scanner error. No test uses those bytes.

**Limit, documented.** A scan that runs nothing cannot know where a program
reads its own text as data (`currentfile eexec`, inline `image` samples,
`readhexstring`); those bytes may be reported as errors.

**Deferred.** A column number; reporting every error rather than the first;
moving pscat-mcp onto the API (its repo); retiring its use of `pscat::lexer`.
