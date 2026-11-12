## `Interp::new_scaled`: a scaled interpreter whose page the program may choose (issue #1097, 2026-11-11)

pscat-mcp built an interpreter for "a dpi but no page size" with
`Interp::with_page_scaled(612, 792, scale)` followed by
`gfx_mut().set_page_size_source(PageSizeSource::Default)`, which reaches into
the `#[doc(hidden)]` `gfx` module. `Interp::new_scaled(scale: f32) ->
Option<Interp>` is the supported spelling: the same page-size semantics as
`Interp::new()` (US Letter until a program's `setpagedevice /PageSize`
replaces it, and the replacement keeps the scale), at `scale` device pixels
per point. `new()` is now `new_scaled(1.0)`.

- **Name and shape.** `new()` has no arguments and the scaled constructors are
  `with_page_scaled`, so `new_scaled` is the pair of `new` as `with_page_scaled`
  is of `with_page`; the issue's other proposal (`with_page_scaled_default`)
  would have taken a page size that the default semantics then ignore.
  `Option`, like every other constructor that can be refused: `None` past
  8000 pixels on a side (Letter, so a scale above about 10.1).
- **Scale guard.** `new_scaled` also returns `None` for a scale that is not a
  positive finite number. `with_page_scaled` does not check that (a NaN or 0
  scale builds a degenerate 1x1 canvas), and is left as it was: its callers
  (wasm scale from JS, `--dpi`) are not audited here.
- **`%%BoundingBox`.** Only the CLI (`src/main.rs`, `--bbox`) reads it; the
  library's `Interp` honours `setpagedevice` alone, so a host that wants
  bounding-box sizing parses it itself (`pscat::bbox`, hidden) and picks the
  constructor from the result.
- `gfx::PageSizeSource` and `Gfx::set_page_size_source` are untouched and still
  hidden; pscat-mcp's pinned build keeps using them until it moves to
  `new_scaled`.
