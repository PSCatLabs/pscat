## lint: UTF-8 text shown in a byte-mode font (issue #1412, 2026-11-26)

`(aπb) show` in Helvetica reads the two bytes of π as two Encoding codes and
draws `aˇ b`. That is correct PostScript, so `--lint` said `clean`. psanim
authors typed `≠`, `→` and `π` into strings and found out only by looking at
the frame. #1425 then gave pscat a way to say it means UTF-8: `/PSCatUnicode
true` in the font dict. This is the lint half.

**What was built.** `[utf8-in-byte-font]`: `a string holding UTF-8 text (π
U+03C0) was shown in the font /Helvetica when the program reached line 3; that
font reads each byte as one character ...; put /PSCatUnicode true in a copy of
the font`. `Interp::trace_utf8_text` (doc(hidden), on under `--lint`, one
branch in `begin_show` when off) notes each such show into a bounded
`lint::Utf8TextTrace` (5 fonts, 6 distinct characters each, the first line).
`begin_show` is where every show operator lands (`show`, `ashow`, `widthshow`,
`awidthshow`, `kshow`, `xshow`/`yshow`/`xyshow`, `cshow`, `stringwidth`,
`charpath`), and `glyphshow`'s one-byte placeholder is never non-ASCII.
`--bands` lints band 0 through `check_band`. `%lint: allow utf8-in-byte-font`
silences it.

**What changed against the issue.** The issue said "single-byte font whose
encoding maps those bytes to `.notdef` or unrelated glyphs" and pointed at
#1022/#1023 or Symbol. After #1425 the test is simply "the font has no
`/PSCatUnicode true`" and the message names that key. The Encoding is not
read: a re-encoded font still gets bytes.

**False positives are worse than misses, so it is narrow.** It fires only when
- the WHOLE string is well-formed UTF-8 with at least one non-ASCII scalar:
  `(caf\351)`, other Latin-1, binary data, a cut-off sequence and a string
  that mixes UTF-8 with a Latin-1 byte are silent (a deliberate miss);
- the font is an outline face (`fid >= 0`), not Symbol or ZapfDingbats (their
  high bytes are symbol codes that can form UTF-8 pairs by chance), not a
  user Type 3 / Type 1 dict, and not a `/FontType 0` dict (pscat has no
  composite fonts yet, #1272, but a program's own two-byte codes could look
  like UTF-8);
- the font has no `/PSCatUnicode true` (a `definefont` or `scalefont` copy
  keeps the key, so it is read the way the show reads it, via
  `outline_unicode`) and no explicit `/PSCatUnicode false` (bytes on purpose).
It reads the font live at the start of the show; a `kshow` proc that switches
font mid-string is not followed.

**Corpus (head `--lint` over all 264 tracked `.ps`/`.eps` files and 115
`postscript` blocks in the Markdown docs).** Two hits, both real bugs, both in
committed art: `examples/dataviz_paper.ps` and `gallery/mochi_denim_blue.ps`
show an em dash (UTF-8 `\342\200\224`) in the TeX Gyre Pagella pack face; rendered alone,
the dash comes out as a blank gap. Fixing them changes renders (tier b), so they are a follow-up,
not part of this change; no gate lints them (`selftest.sh` lints only
`selftest/drivers/`), so nothing goes red. No hit in `lib/` or a driver.

**Render identity.** Two binaries (base `f6286a7b`, head), 173 files (every
tracked `examples/`, `gallery/` and `selftest/drivers/` file), 212 pages
byte-identical, 4 files failing identically in both trees skipped; again with
`--pscat-args "--lint"` so the tracing path itself is exercised. The control
is a `/probe` marker in the base tree's `lib/artkit.ps`: the change is only
visible under `--lint`, so no render can tell the binaries apart.

**Not done.** The CLI `--lint`, `--bands --lint` and this crate's `pscat mcp`
server (which runs the `--lint` child) get the finding. A host that builds its
own `Interp` (psanim's `check`, the separate pscat-mcp repo) gets it only by
calling the doc(hidden) `Interp::trace_utf8_text()` before the run. The message
names the dict's `FontName`, which a `definefont` copy keeps from the original
face. Type 3 fonts are skipped.
