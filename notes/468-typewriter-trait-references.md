## Typewriter reference search: two-colour bleed, embossing, worn lane, electric (issue #468, 2026-10-15)

TYPEWRITER.md Q3 asked for public-domain scans of three traits the reference
board lacks; #538 added a two-colour page with a red run and a worn-lane page.
**No qualifying scan turned up for any of the four**, so nothing was cropped
from a new page. `docs/dev/design/typewriter/REFERENCES.md` ("Search for the
unsourced traits") has the full record; this note has the numbers for #538 and
what to do next. No `lib/` file changed.

### What was done

- About 30 Wikimedia Commons file searches, three categories, the Library of
  Congress search API, Internet Archive metadata search and four web searches.
- A hue scan of about 700 NARA/DPLA colour page thumbnails on Commons, ranked by
  saturated-red fraction, top 40 or so inspected: every red mark was a stamp,
  letterhead, form, pencil, seal or coloured sheet. None was typed in red.
- NBS circulars C431, C95 and C186 read in full text. C431 pp. 10-13 is the one
  public-domain statement about bichrome ribbons and the worn band (quoted in
  REFERENCES.md).
- Stem darkness against height measured on Szilard, Duncan and MFR 1955
  (`docs/dev/design/typewriter/tools/lane.py`, `refs/T16_stem_profile.png`).

### Numbers for #538

- Worn lane: stems are 2-5% **lighter** 6-7 pt up than at 2-4 pt (Szilard -2%, Duncan
  -4%, MFR -5%; 395, 45 and 129 stems), not darker. No scan is grey in the middle.
  `/WornBand` has nothing to be tuned against; a realistic preset should stay
  within a few per cent (`/WornBand` 0.3 is about 4.5% of the lane's ink). The sign
  of the model (dark tops) is a design choice.
- Two-colour: C431 says most bichrome ribbons show no diffusion; where there is
  any, black spreads into red; some ribbons leave a white "pin stripe". So keep
  `/Bleed` default 0 and small in presets, and try an opt-in boundary gap of one yarn
  (0.48 pt, 40 glyph units at 12 pt) beside the blend. `tBlTr` (60 units a side) is
  already about three yarns. The depth constant in the code is `tBleed` x 540, not
  the 350 in #538's text. For a red run, a black-tinted head or foot has
  support; a red foot on black text only has the mechanism.
- Ink colour of a worn ribbon: Szilard's darkest 1% of pixels average RGB
  (76, 68, 64), a neutral dark grey-brown, not purple. C431's purple is a property
  of dye-heavy oily ink that was not seen on the board.
- Electric: MFR 1955 measures mean darkness 0.65, per-letter spread 0.019, coverage
  CV 0.17 at 200 dpi, no more even than manual Duncan, but the proportional face
  and the resolution make it a poor comparison. `/electric` is left as it is.
- Embossing: no raking-light, verso or transmitted-light scan exists on the board.

### Licence reasoning

Nothing new is committed from a source page. `refs/T16_stem_profile.png` is a plot
computed from three scans already on the board (Duncan and MFR 1955 `{{PD-USGov}}`,
Szilard `{{PD-US-no notice}}`); it contains no pixels of them. The C431 quotations
are from a US government publication (17 U.S.C. 105).

### Follow-up

A photograph or scan of a page typed by the project's owner on a named machine,
with a two-colour ribbon and a known Selectric, would settle three of the four
traits and has none of the licensing problems. Filed as #551.
