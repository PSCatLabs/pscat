# #1160: chalk supports `\$ \% \& \# \_`

`pscat chalk` refused TeX's escaped literal characters (`\$5`, `50\%`, `R\&D`,
`a\_b`, `\#1`). Each is now the character itself, an ordinary atom, in math and
inside `\text{}`, `\mathrm{}` and `\operatorname{}`. `\{ \}` already worked in
math; they now work in `\text{}` too.

- **Glyphs.** strokeglyphs had no `$ % & # _`, so five single-stroke glyphs were
  authored: `/dollar /percent /ampersand /numbersign /underscore` (digit height;
  the underscore hangs below the baseline). They are in `sgsrc` only, not in
  `sgcmap`: mathkit reaches them by name from the backslash escape, so a bare
  `$ % & # _` in `sgpath` text, and in `\text{}`, is still a missing glyph.
  That keeps every existing `sgpath` string byte-identical, and a bare `_ & #`
  in `\text{}` an error, as in TeX. (ASCII `sgpath` support would be a separate,
  visible change.)
- **mathkit.** `mkctlsym` (control symbols in math) looks the byte up in
  `mkescsym`; `mktxcmd` (a backslash inside `\text`) in `mktxesc`, which adds
  `\{ \}`. `mkrawarg`, which finds the end of a `\text{...}` group, now skips the
  byte after a backslash, so `\text{a\}b}` does not end at the escaped brace.
- **Rust.** Nothing in `src/chalk.rs` needed to change: comment stripping,
  `has_delim` and `unwrap_math` already pair a backslash with the next byte, and
  rows split only at `\\`. Pinned by a unit test (`R\&D` is one row, `$\$5$`
  unwraps, `50\% % note` keeps the percent and drops the comment).
- **Docs.** `pscat chalk --help` lists the literals; the mathkit header and the
  dry-media guide say so.
