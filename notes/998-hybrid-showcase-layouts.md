## Hybrid showcase: the pscat layouts redesigned until they beat the image-model-only panels (issue #998, 2026-11-05)

**Why.** In #995's comparison sheets the image model's whole posters (a)
looked bolder and more finished than pscat alone (b) and the hybrid (c). The
owner's reading: (a) is generic AI output, and if our layout looks weaker we
have not worked on it enough. This was a design pass, not an engine change.

**What changed.**
- **Poster (`sky_talk.ps`).** A stated six-column grid on 44 pt margins.
  Title in Abril Fatface (a fat Didone) at 98 pt, set solid; Palatino for
  the reading lines; every label in tracked Jost (`ashow`). The formula is
  a typeset fraction at 40-46 pt, the chart spans four columns with 13 pt
  values, and both sit in one band under a hairline rule. A faint shade
  (strips of rising `setalpha`) under the science band and a darker one under
  the date, venue and QR on the sea. The marks in the science band are drawn
  twice, a soft dark shadow then white, so labels hold over bright cloud.
- **Menu (`menu_card.ps`).** Playfair Display name; Cinzel (capitals only,
  so the copy stays as typed) for the address and section heads, tracked,
  with hairline rules to the price column; Palatino dishes with dot leaders
  to right-aligned prices on R = 316 pt, which stops where the painted leaves
  begin. The QR's light modules take the paper's tone under it, so there is
  no white box, and the caption sits level with the code's middle.
- **pscat alone (b).** Not paintkit imitating the painting: the poster's sky
  is a screen-printed vector sky (1 pt gradient strips, three-ink clouds scaled
  towards their crowns, tapered stratus, a low sun with streaked reflection
  kept above the date, fine grain); the menu is a flat-ink botanical print on
  paintkit's paper (shadow side, graded blush, speckle, highlight, a key line
  a point off register). The owner's steer: play to pscat's strengths.
- **Test.** `tests/hybrid.rs` now captures `ashow` as well as `show`, since
  the tracked labels are drawn by `ashow`. The copy lists, string counts and
  every other check are unchanged.

**Fonts.** Abril Fatface, Jost, Playfair Display and Cinzel are catalog
faces (`extras` pack). A checkout has them; an installed pscat needs
`pscat fonts install --pack extras` or they stand in as Helvetica with a
one-line note. Said in each program's header and the examples README.

**Blind reviews.** Two fresh agents, each shown the three posters and the
three menus unlabelled and shuffled, no context:
- Round 1 (after the first pass): poster (c) first, (b) second, (a) last;
  menu (c) first, (a) second, (b) last, and on layout (a) ahead of (b) = (c).
  Their notes drove round 2: a larger speaker line and labels, a larger QR
  caption, reflection streaks off the date, the menu's type larger and its
  sections spaced, the address darker, the QR box removed, the leaves kept on
  the page.
- Round 2: poster (c) > (b) > (a), layout (b) = (c) > (a); menu (c) > (b) >
  (a), layout (b) = (c) > (a). Remaining notes: (b)'s art reads as clip art
  to a critic; the menu's upper right is empty paper and the pancakes line
  gets a single leader dot.

**Not done.** No new background images (they are frozen assets); no engine or
library change.
