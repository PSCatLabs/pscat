# 1182: pin the compiled-in handwriting library, and check glyphs without PostScript

Requested by pscat-mcp. Its worker has a `lib/handscript.ps` in `$PSCAT_HOME`
(the font-pack directory) and runs with a working directory of `/`, both of
which `paths::open` consults before the compiled-in copy, and its server process
never runs PostScript.

API (all in `pscat::handwriting`):

- `enum Library { Resolved (default), Embedded }`, `#[non_exhaustive]`, `Copy`.
- `HandOptions::library: Library`. `measure_source` and `render_source` carry
  that library's text. `HandOptions` is `#[non_exhaustive]` and `Default`s the
  field to `Resolved`, so existing code and results are unchanged.
- `has_glyph_in(Library, char)` and `drawable_in(Library, &str)`. `has_glyph` and
  `drawable` are the same with `Resolved`. The comment on the issue asked for
  one choice to cover all three, so an embedder sets `opts.library` once and
  passes `opts.library` to the glyph functions.

Decisions:

- `Embedded` never falls back to `(lib/handscript.ps) run` (that would let the
  interpreter go looking on disk); it `expect`s the embedded copy (`build.rs`
  embeds every `lib/**/*.ps` and `Cargo.toml` ships them, so a build has it; a
  unit test checks). The fast path in `glyph_table_for` uses `paths::embedded`
  without `expect`, so the existing `has_glyph`/`drawable` cannot panic.
- The glyph set of the compiled-in font is a checked-in const,
  `EMBEDDED_GLYPHS` (a sorted string of characters: readable, diffable). A
  unit test runs `discover_glyphs` on the embedded text directly (not through
  `glyph_table_for`, which short-circuits on that text) and fails with the
  literal to paste when they differ.
- `glyph_table_for` takes the same shortcut for any library text that is
  byte-equal to the embedded copy, so even `Resolved` runs no PostScript when
  the resolver lands on it (the usual case). Overrides still use bounded
  discovery with the cache.
- "Runs no PostScript" is tested with a thread-local counter, test-only, at the
  one place this module builds an `Interp` (`discover_glyphs`). The test first
  proves the counter can move (a text no other test uses must build one
  interpreter), then that `Embedded` calls, and `Resolved` on the compiled-in
  copy, leave it unchanged.
- `tests/handwriting_pinned.rs` (own binary, one test) puts different fonts
  (one without `.`, one without `,`) in `$PSCAT_HOME` and in the working
  directory. `Resolved` follows each in turn; `Embedded` follows neither, for the
  render and measure programs and the glyph checks; the pinned measuring
  program also runs.
- The module doc now lists the resolution order and says that read roots and
  limits do not apply to this read. `src/paths.rs` is untouched.
