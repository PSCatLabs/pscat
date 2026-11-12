# 982: PNG export writes a pHYs chunk

`pscat --dpi 300 --png card.png card.ps` used to write a PNG with only `IHDR`,
`IDAT` and `IEND`, so print dialogs and image tools assumed 72 or 96 dpi and
printed a 3.5 in card at 11 to 14 in.

Every PNG now carries one `pHYs` chunk: unit 1 (metre) and
round(dpi / 0.0254) pixels per metre on both axes (72 dpi 2835, 96 dpi 3780,
144 dpi 5669, 300 dpi 11811).

- `src/png.rs` takes tiny-skia's encoded PNG and inserts the chunk before the
  first `IDAT`. Nothing else in the file changes, so the decoded pixels are
  identical by construction (the unit test and `tests/png_phys.rs` check it).
- The resolution is a *scale* (dpi / 72, the f32 every canvas is built with),
  not a dpi, so `Page::to_png`, `--png`, `--sweep`, `--bands` and the MCP
  tools all derive the figure from the same number and stay byte-equal to each
  other (`tests/sweep_identity.rs`, `tests/bands.rs`).
- Contact sheets from `--contact-sheet` use `--dpi` (their cells are unscaled
  renders). `pscat sheet` and `review_sheet` put cells of different dpi side by
  side with captions, so they record a nominal 72 dpi, one pixel to the point.
- `Pixmap::save_png` and `encode_png` are in `clippy.toml`'s
  `disallowed-methods`; `png.rs` is the allowed caller. PNGs embedded in an
  SVG `<image>` (`src/image.rs`) are not files pscat writes and keep the plain
  encoder.
- `pscat-mcp` runs the CLI for `render_postscript`, `start_render` and
  `handwrite`, so they follow from the CLI. `chalkboard_math` uses
  `Page::to_png` (144 dpi by default, 5669 px/m).
- `scripts/identity_check.sh` compares the committed `alpine_lake.png` still
  with `pHYs` dropped from both sides: stills committed before this change
  have no chunk and are not regenerated here.
