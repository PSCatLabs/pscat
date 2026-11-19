## A missing glyph advances by a space on the Liberation faces (issue #1115, 2026-11-18)

An unknown glyph name (`show` of an unencoded byte, `glyphshow` of a name the
face lacks) advances by `.notdef`'s width. Liberation's `.notdef` is a box
(750 units in Sans, 778 in Serif, 600 in Mono), so text with a missing glyph
spaced out further than in Ghostscript. Noted in `notes/1022-definefont-copy-keeps-face.md`.

Measured with Ghostscript 10.07 (correctness only) over all 35 standard names,
`1000 scalefont`, an unknown glyph name via `glyphshow` and `(\001) stringwidth`
(the two agree on every face). gs's `.notdef` is as wide as the face's space:

| face family | gs | pscat before | pscat now |
|---|---|---|---|
| Helvetica (4) | 278 | 750 | 277.8 |
| Times (4) | 250 | 777.8 | 250 |
| Courier (4) | 600 | 600.1 | 600.1 |
| Palatino (4) | 250 | 500 | 250 |
| Bookman (4) | 340, 340, 320, 300 | 280 | same as gs |
| New Century Schoolbook (4) | 278, 287, 278, 287 | 280 | same as gs |
| Avant Garde (4) | 277, 277, 280, 280 | 500 | same as gs |
| Helvetica-Narrow (4) | 228 | 280 | 228 |
| Zapf Chancery | 220 | 280 | 220 |
| Symbol, ZapfDingbats | 250, 278 | same | unchanged (URW faces) |

After the change `pscat` prints the same 23 numbers as gs for the faces that are
not Liberation (a diff of the two outputs is empty), and the Liberation twelve
agree within font-unit rounding (277.83 for 278).

The issue suggested 250 everywhere; gs does not do that (Helvetica is 278), so
the rule is gs's: on the standard 35 (the twelve `BUILTINS` faces and the
`EMBEDDED` TeX Gyre faces) a missing glyph takes the `space` glyph's advance
(`font::missing_glyph`). Every other face (user and pack fonts, Type 1 and
Type 3) keeps its own `.notdef`. Present glyphs, and what is drawn for a
missing one (nothing), are unchanged, so no gallery render moves unless it
shows an unknown glyph: the two-binary identity check over the tracked
examples and gallery pieces was identical.
