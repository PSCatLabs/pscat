## SVG export: the root width and height now carry `pt` (issue #981, 2026-11-03)

Part of #957 (the QR epic), fixed on `feature/qr-code-library` because the
QR tests parse the SVG header.

**Change.** `src/svg.rs` wrote `width="252" height="144" viewBox="0 0 252 144"`.
SVG reads a unitless root length as CSS px (96 per inch), so a 252 x 144 pt
(3.5 x 2 in) card opened in browsers, Inkscape and print dialogs at
2.625 x 1.5 in. The root now reads `width="252pt" height="144pt"`. The
`viewBox` and everything drawn inside it stay in page units (points), so the
drawing is unchanged; the white backdrop `<rect>` is in user space and stays
unitless. Anyone who reads the SVG header sees the difference (CHANGELOG
entry comes from the PR title at release).

**Checking raster tools.** usvg converts `pt` to px at its `dpi` option, 96 by
default: a 252pt page becomes a 336 px tree, 4/3 too big for a test that wants
`scale` pixels per point. `tests/qr_support/mod.rs::raster_svg` now parses
with `dpi: 72.0` so 1 pt = 1 px and the QR raster tests keep their pixel
sizes. Any other test rasterizer that reads pscat's SVG must do the same.

**Not affected.** The browser playground and the WASM build draw PNG pixels to
a `<canvas>`; neither embeds the SVG export, so there is no on-screen size
change.
