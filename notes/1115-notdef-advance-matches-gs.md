## A missing glyph advances by a space on the Liberation faces (issue #1115, 2026-11-18)

An unknown glyph name (`show` of an unencoded byte, `glyphshow` of a name the
face lacks) advances by `.notdef`'s width. Liberation's `.notdef` is a box
(750 units in Sans, 778 in Serif, 600 in Mono), so text with a missing glyph
spaced out further than in Ghostscript. Noted in `notes/1022-definefont-copy-keeps-face.md`.

Measured with Ghostscript 10.07 (correctness only), `1000 scalefont`, an
unknown glyph name, and `(\001) stringwidth`; the two agree on every face:

| face family | gs | pscat before | pscat now |
|---|---|---|---|
| Helvetica (4) | 278 | 750 | 277.8 |
| Times (4) | 250 | 777.8 | 250 |
| Courier (4) | 600 | 600.1 | 600.1 |
| Symbol | 250 | 250 | 250 (unchanged, URW face) |
| ZapfDingbats | 278 | 278 | 278 (unchanged, URW face) |

The issue suggested 250 everywhere; gs does not do that (Helvetica is 278),
so the rule is gs's: a URW `.notdef` is as wide as that face's space. On the
twelve `BUILTINS` faces a missing glyph now takes the `space` glyph's advance
(`font::missing_glyph`); the Liberation space widths are the Adobe ones
(569/2048 and 512/2048 em, 278 and 250 after rounding). Every other face
(Symbol, Dingbats, TeX Gyre, catalog and user fonts, Type 1) keeps its own
`.notdef`. Present glyphs, and what is drawn for a missing one (nothing), are
unchanged, so no gallery render moves unless it shows an unknown glyph.
