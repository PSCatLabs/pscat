# 1170: handwriting::has_glyph and drawable

`pscat::handwriting::has_glyph(c: char) -> bool` and
`drawable(text: &str) -> bool` let an embedder (pscat-mcp) refuse text that
would draw nothing, without keeping its own copy of the font's character set.

- The answer is derived, not listed. `has_glyph`/`drawable` take the library
  text `library()` returns on that call (the same text `render_source` would
  embed, `$PSCAT_HOME` first) and run a short PostScript program after it in
  a throwaway `Interp`. For each code 0..127 it reads the name in `/Encoding`,
  falls back to `.notdef` when `/CharDefs` lacks it (as `BuildChar` and the
  wrap do), and counts the code when the glyph has a stroke beyond its advance.
- Discovery is bounded (round 1 review): 5M steps and 2 s, plus a 64 MiB
  heap-growth cap when the process has `TrackingAllocator` installed (round 2;
  `set_limits` refuses a memory cap without it), through `Interp::set_limits`, and `set_read_roots(&[])` so the program reads nothing
  but the embedded lib. A library that loops, errors or reads a file gives "no
  glyphs" (all false), never a hang or panic.
- A memory-limit stop is not cached (the heap count is process-wide, so
  another thread could have caused it); other failures are cached for that text.
- The cache is keyed by a hash of the library text and each entry keeps the
  text, compared on a hit so a collision cannot return another font's table, in
  a `Mutex<HashMap>` of at most 8 entries that is recovered if poisoned and not
  held while discovery runs. A changed override, or a different working
  directory resolving a different file, therefore changes the answer exactly
  when it changes what `render_source` embeds; a failed discovery is cached
  only for that text. Cost: each call re-reads the library like `render_source`
  does, so `drawable` fetches the table once per string.
- `space` and `.notdef` are only an advance, so `has_glyph(' ')` is false.
  Letters are lowercased first (the programs lowercase ASCII); non-ASCII is
  false (its UTF-8 bytes land on `.notdef`).
- `tests/handwriting_api.rs` renders each printable ASCII character through
  `render_source` and asserts ink if and only if `has_glyph`, so a font change
  cannot make the two disagree unnoticed. No character list is pinned.
  `tests/handwriting_override.rs` (own binary, sets `PSCAT_HOME` once) checks a
  runaway override gives false promptly, a good one then works, and a changed
  one changes the answer, and a memory-bomb override stops at the cap (that
  binary installs `TrackingAllocator`); a unit test covers the same by library text.
- Not done: a helper listing the unsupported characters of a string (not
  asked for).
