## Two committed pieces drew an em dash as a blank gap (issue #1445, 2026-11-26)

`examples/dataviz_paper.ps` (the subtitle and four chart captions) and
`gallery/mochi_denim_blue.ps` (the painted signature) wrote an em dash as UTF-8
inside a string shown in a Palatino face. Those faces read one byte per
character, so the three UTF-8 bytes were three unencoded codes and the dash drew
as a blank gap (since #1115, one space wide). `pscat --lint` finds it as
`[utf8-in-byte-font]` (#1412).

- **Fix.** Each shown dash is the octal escape `\320`, the em dash in
  StandardEncoding, which is the encoding of the Palatino faces (`/Palatino
  findfont /Encoding get 208 get` is `/emdash`; `(\320) stringwidth` at 10 pt is
  10 wide). The other text in both pieces is plain ASCII, so a byte escape
  matches how they set text, and it draws the same in Ghostscript, which ignores
  `/PSCatUnicode`. A string with a real non-Latin-1 character would use
  `/PSCatUnicode true` instead (docs/guides/fonts.md, "Unicode text and emoji").
  The `%%Title` comment lines keep their UTF-8 dash: a comment is never shown.
- **Renders.** `gallery/renders/mochi_denim_blue.png` is regenerated
  (README recipe: `2 2 scale` prepended, `--page 1440x1800`). `dataviz_paper`
  has no committed render. Against a same-binary render of the base tree only
  the dash lines differ (mochi: the signature strip; dataviz: the subtitle and
  four caption lines, each re-centred around the wider dash). The previously
  committed mochi still had drifted from what the base tree renders (24668 of
  2.6M pixels, off by at most 2 of 255, spread over the page) because earlier
  interpreter changes did not regenerate it, so the regenerated PNG carries that
  too. It is below what the eye can see.
- **Not done.** A CI lint step over tracked `.ps` files so this cannot return
  (the issue says propose it, not do it). Two caption lines in `dataviz_paper`
  ("Bar chart" and "Line chart") sit over the bars and axis labels; that layout
  was already so and is not touched here.
