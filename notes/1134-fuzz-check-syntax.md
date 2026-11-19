# fuzz: a target asserting check_syntax never panics (#1134)

`fuzz/fuzz_targets/check_syntax.rs` feeds raw bytes to
`pscat::syntax::check_syntax`, the public validator a host runs on untrusted
programs (pscat-mcp's `validate_postscript`). A panic would take down the
host's worker instead of returning a syntax error.

- Seeds: `fuzz/seeds/check_syntax` is a symlink to `fuzz/seeds/lexer`, so the
  lexer seeds (including the #1103 binary-object seeds) are used with no copy
  to drift.
- Crashes: `fuzz/regressions/check_syntax/` is where a fixed crash goes;
  `tests/fuzz_corpus.rs` replays it (`check_syntax_seeds_do_not_panic`).
- CI: `check_syntax` is a row of the `fuzz` workflow matrix.
- No change to `src/` (so no overlap with #1309 in `src/fuzzing.rs`).
