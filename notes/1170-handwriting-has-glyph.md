# 1170: handwriting::has_glyph and drawable

`pscat::handwriting::has_glyph(c: char) -> bool` and
`drawable(text: &str) -> bool` let an embedder (pscat-mcp) refuse text that
would draw nothing, without keeping its own copy of the font's character set.

- The answer is derived, not listed. The first call runs a short PostScript
  program after `library()` (the same `lib/handscript.ps` text `render_source`
  embeds, `$PSCAT_HOME` first) in a throwaway `Interp`. For each code 0..127 it
  reads the name in `/Encoding`, falls back to `.notdef` when `/CharDefs` lacks
  it (as `BuildChar` and the wrap do), and counts the code when the glyph has a
  stroke beyond its advance. The table is cached in a `OnceLock`.
- `space` and `.notdef` are only an advance, so `has_glyph(' ')` is false.
  Letters are lowercased first (the programs lowercase ASCII); non-ASCII is
  false (its UTF-8 bytes land on `.notdef`).
- If the table cannot be read, every character is false rather than a panic.
- `tests/handwriting_api.rs` renders each printable ASCII character through
  `render_source` and asserts ink if and only if `has_glyph`, so a font change
  cannot make the two disagree unnoticed. No character list is pinned.
- Not done: a helper listing the unsupported characters of a string (not
  asked for).
