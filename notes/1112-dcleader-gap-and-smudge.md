## dcleader: a clear gap after the text, dots that hold over smudges (issue #1112, 2026-11-19)

PR #1076's perceptual review found two things in the menu specimen: a row's
first leader dots could sit right against the last letter ("Croissant"), and
dots over a smudge or swipe faded almost to nothing.

**Reference.** Real chalk menu boards (Wikimedia Commons, "Menu Chalkboard, Le's
Baguette Banh Mi Cafe, New Orleans 01" and "Cafe menu (Unsplash)") leave about a
word-space between an item and its price, and the filler dots on the boards
(the `. PORK .` marks) are pressed about as bright as the lettering, not
fainter. Neither photo has a long dotted leader, so this is a reference for the
spacing and the brightness of a pressed dot, not for a leader's pitch. The old
defaults (a gap of 0.3 of the size, dots at 0.8 of the lettering's strength)
were on the faint side of that.

**Change.**
- `/Gap` defaults to 0.5 of `/Size` (was 0.3). The dots still sit on whole
  multiples of `/Pitch` from x = 0, so rows keep their dots lined up; only the
  nearest dots to each string are dropped. A row too short for dots still draws
  none.
- The dots are 0.85 of the lettering's width (was 0.8) and its strength plus 0.15,
  capped at 1 (was 0.8 of it). Strength is a blend from the board toward the
  chalk, and a smudge lightens the board, so the same blend reads closer to the
  ground there; a stronger dot keeps its contrast. `/Leader` still overrides
  either.

**Measured.** In `examples/drychalk_hand.ps` at 144 dpi, the dot peak minus the
background around it (a 7 x 7 px peak against the median of the rows above and
below), averaged over a third of each row. Every row passes under the eraser
swipe at some point, so the figures are for the whole menu, left to right:

| row region | before | after |
|---|---|---|
| right third (clear of the swipe) | 68 to 82 | 113 to 128 |
| middle third (the swipe and smudge) | 62 to 72 | 105 to 119 |
| left third (the swipe and its edge) | 55 to 72 | 66 to 106 |

Cinnamon roll, left third, is the weakest after the change (55 to 66): the
densest part of the smudge streak still reads a little dimmer than clean board,
which the perceptual reviewer noted as polish.

**Structural test.** `dcleader-keeps-its-dots-clear-of-the-lettering` reads the
first and last dot indices left in drychalk's scratch dict and checks the gap
from `sgwidth`, a given `/Gap` and `/Pitch`, and that
`/Strength` 0.95 and 1 raise no range error (no render hash, so the 16-bit
compositing change of #1306 cannot break it).

**Known side effect.** Lettering drawn at `/Strength 0` now gets dots at 0.15
(the lettering's strength plus 0.15), as documented.

**Not done.** A slanted row's last letter leans right at its top; the gap is
measured from the advance, not the ink, so a heavily slanted row sits a little
nearer than an upright one.
