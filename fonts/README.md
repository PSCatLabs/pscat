# Bundled fonts

Liberation Fonts 2.1.5 (Sans, Serif, Mono — regular/bold/italic/bold-
italic), from
<https://github.com/liberationfonts/liberation-fonts/releases/tag/2.1.5>,
licensed under the SIL Open Font License 1.1 (see `LICENSE`).

They back the standard PostScript base-font names (Helvetica, Times,
Courier families) — Liberation faces are metrically compatible with
those, which is what document layout depends on. See `docs/dev/FONTS.md` at the
repo root for the architecture. The files are compiled into the binary
with `include_bytes!` (`src/font.rs`), so nothing here is read at
runtime.

The other 23 standard names (Palatino, Bookman, New Century Schoolbook,
Avant Garde, Zapf Chancery, Helvetica-Narrow, Symbol, ZapfDingbats) are
compiled in the same way from `base35/` (TeX Gyre and URW; see
`base35/README.md`). `catalog/` is the font catalog, loaded from disk
and shipped as font packs.
