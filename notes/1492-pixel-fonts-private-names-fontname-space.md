## Pixel fonts: one name left behind, a /FontName, one /space rule (issue #1492, 2026-11-27)

psanim's first real use of the #1426 faces (psanim PR #114) found four rough
edges. Three are fixed here, in `lib/fonts/phosphor.ps`, `pinpoint.ps`,
`diode7.ps` and `diode5.ps`; the fourth waits on the owner.

### What changed

- **Names.** Each file built its font in a named dict (`PhosphorDict`,
  `PinpointDict`, `Diode7Dict`, `Diode5Dict`) and defined `pf-variant`, both in
  the dictionary current at run time. The font dict is now built anonymously
  (`40 dict begin ... currentdict end /Name exch definefont pop`), so the
  `*Dict` names are gone and `pf-variant` is the only name a file leaves. It
  keeps its name, its `/New /Base << dials >> pf-variant` form and its place:
  psanim's `pixelvariant` calls it by name from the private dictionary it ran
  the file in. The renderer's helpers (`work`, `on`, `prep`, `dotpath`, ...)
  always lived in the font dict and still do. No second, library-private name
  was added: `pf-variant` calls only operators, so `//name exec` has nothing
  to protect, and another name would be one more leak. A program with its own
  `pf-variant` runs the file in a dictionary (documented in each file's header
  and in docs/guides/fonts.md).
- **/FontName.** Every face's dict carries `/FontName`, its registered name.
  `pf-variant` sets the copy's `/FontName` to the new name (after the dials,
  so a `/FontName` in the dials does not win); before, the copy would have
  inherited the base's. pscat reads `FontName` only for message text (the
  UTF-8-in-a-byte-font note), so a message about these faces now names them
  instead of `?`.
- **/space.** `[]` on /Phosphor and /Pinpoint-*, `[(..)]` on /Diode7 and
  /Diode5. Now one row of `.` everywhere, as wide as the face's glyph box (the
  advance less the dark gap after a glyph): `[(.......)]` (7: the 8-column
  cell less its dark last column, the width of almost every row in those
  files) on the monospaced faces, `[(..)]` unchanged on the Diode faces
  (2 + `/Gap` 1 = 3). The prepared glyph is the same (`[8 [] [] []]` before
  and after), so nothing renders differently. 7, not 8: psanim's
  `pkpixel.ps` turns `[]` into one column padded to `/Cell` - 1 = 7 and takes a
  glyph's width from its first row, so a 7-dot row keeps psanim's space as it
  was, where an 8-dot row would have widened it by a column.

`/Bitmaps` is otherwise unchanged and the six registered names
(/Phosphor, /Phosphor-Scan, /Pinpoint-Draft, /Pinpoint-NLQ, /Diode7, /Diode5)
are unchanged. `.notdef` still differs between faces (`[]` against `[(...)]`);
it was out of scope here.

### Not changed: findfont before run

`/Phosphor findfont` before the file runs gives the Helvetica substitute under
/Phosphor, silently, and `FontDirectory /Phosphor known` is then true. The fix
(auto-load by name, or a warning when `findfont` substitutes for one of these
names) is in `src/` and waits on the owner's pixel-font naming decision. The
docs say "run the file first"; the question goes in the 0.8.0 owner list.

### Evidence

`scripts/identity_check.sh --files` over `examples/retro_fonts.ps` with a
control, plus a per-face strip with spaces: byte-identical (numbers in the PR).
Load cost (psanim, loaded machine): 13 to 30 ms for `phosphor.ps`; docs now say
load in setup.
