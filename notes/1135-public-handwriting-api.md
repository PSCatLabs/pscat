## Public handwriting API, `pscat::handwriting` (issue #1135, 2026-11-12)

What was built. The program generation behind `pscat handwrite` moved from
`src/subcmd/handwrite.rs` into a supported library module, `src/handwriting.rs`
(PR for #1135). `HandOptions` (non_exhaustive, `Default`, `new`), `Paper`,
`HandError` (non_exhaustive enum, `code()` and `field()`), `validate()`,
`measure_source`, `parse_line_count`, `page_height_pt` and
`HandOptions::height_pt`, `render_source`, and the `DEFAULT_`/`MIN_`/`MAX_`
consts. The functions return source and run nothing, so the caller (pscat-mcp's
worker) checks the pixel cap and height budget between the two passes and runs
each under its own limits. The subcommand parses its flags into a `HandOptions`
and calls the same functions. No tiny-skia in any public type.

Choices.
- The programs still inline `lib/handscript.ps` as `paths::open` finds it,
  exactly as the subcommand did, rather than `(lib/handscript.ps) run` as
  `pscat::chalkboard` does. A `run` is resolved by the interpreter, so it would
  fall under `--read-root`/`--confine-reads`, `--max-steps` and the error line
  numbers, and the resolution would move from generation time to run time. The
  price is a 15 kB source.
- Numbers are typed (`f64`, `u32`, `i32`) and written with Rust's `{}`, where the
  flags used to paste the user's text (`--size 36.0` put `/Size 36.0` in the
  program, now `/Size 36`). A sweep of 44 option sets over the base and head
  binaries (CRLF, lone CR, parens and backslashes, trailing newlines, non-ASCII,
  `--height` given and auto, ruled paper, 36 and 144 dpi, halftone, `36.0`,
  `.5`, `24.`) gave identical PNGs.
- `height_pt(lines)` honours the options' margin and leading; the free
  `page_height_pt(lines, size)` is the same at the defaults.
- There is no slant option in the subcommand or the font (a per-glyph slant is
  random inside `lib/handscript.ps`), and no drawable-character filter: the font
  advances an unknown character as a blank, so removing one would change the wrap
  and the bytes. Neither was invented.

Intended changes to the subcommand, all on inputs that were already odd:
- Ranges are now enforced (`validate()`): `--size` 0..2000, `--margin` 0..8000,
  `--leading` 0..100, `--pen` 0..1000, `--ink` components 0..1, `--jitter` 0..10000.
  A negative size, margin, leading or pen, an ink component above 1 and a size
  over 2000 used to render (or fail on page height) and are now usage errors.
- `--jitter` and `--seed` take whole numbers: `--jitter 16.5` and `--seed 1.5`
  used to fail mid-render with a typecheck (`mod`, `srand`); `--seed` over the
  32-bit range, which pscat accepted but Ghostscript does not, is refused.
- A missing `lib/handscript.ps` no longer reports "missing library file"; the
  embedded copy always exists, so that cannot happen on a normal build.

Evidence. `tests/handwriting_api.rs` runs the subcommand and the library path
(validate, measure, height, render on an `Interp`, `Page::to_png`) for five
option sets and compares the PNG bytes; the module has a doctest of the server
flow.

Deferred. pscat-mcp's own `handwrite` tool still shells out to the subcommand;
moving it to the library is its repository's change.
