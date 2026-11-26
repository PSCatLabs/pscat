## Retro terminal, dot-matrix and LED pixel fonts (issue #1426, 2026-11-26)

Four new `lib/fonts/` files, original pixel designs drawn for pscat (not copied
from any terminal, printer or LED controller ROM, nor any existing font), for
psanim's terminal, dot-matrix printer and LED marquee pieces (psanim#47, #55,
#56, #60):

- `/Phosphor` and `/Phosphor-Scan` (`phosphor.ps`): an 8x16 terminal cell, cap
  height 10, x-height 7, descenders 3, two-pixel stems, a dotted zero; `-Scan`
  has `/ScanGap 0.45`.
- `/Pinpoint-Draft` and `/Pinpoint-NLQ` (`pinpoint.ps`): nine pins by eight
  columns, one-dot strokes, a slashed zero; NLQ adds dots halfway between
  every pair of neighbouring dots (8-neighbourhood) at a smaller size.
- `/Diode7` (`diode7.ps`): 5x7 caps, x-height 5, one descender row,
  proportional with one dark column after each glyph.
- `/Diode5` (`diode5.ps`): five rows, capitals, digits and punctuation, widths
  1-5, a-z mapped to capitals.

Each glyph is a list of `#`/`.` rows in the file (the source; no generator).
A shared renderer turns them into runs, dot centres and in-between points at
load; BuildChar fills one path in the current colour. Dials (`/DotShape`,
`/DotSize`, `/ScanGap`, `/Interpolate`) live in the font dict and variants are
copies (`pf-variant`), so gs's glyph cache stays right; design notes are in
`docs/dev/FONTS.md`. Registered as `Type3Face` rows in `src/capabilities.rs`
(so `--capabilities` and `describe_art_capabilities` list them), and `pscat
fonts list` gained a `type3 face:` line per Type 3 face with its `run` line.

Like the other `lib/fonts/` faces, a program runs the file before `findfont`;
auto-loading a `lib/fonts/` face from a bare `findfont` is left to the owner
(it would change `findfont`'s resolution, which issue #1425 is also working on).

Tests: `tests/retro_fonts.rs` (every printable ASCII code draws, bitmaps fit
their cells, square marks at true size match `/Bitmaps` pixel for pixel, the
caller's colour, scan gaps and NLQ change the ink, `pf-variant`, the renderer
copies agree, gs accepts every face, the specimen renders) and a `fonts list`
test in `tests/font_packs.rs`. Specimen: `examples/retro_fonts.ps`,
`docs/dev/design/retro_fonts/specimen.png`.

Deferred: a 10x20 terminal cell, a block-cursor glyph, unlit-LED backgrounds,
glow (psanim's side), a gallery `specimens` entry (#581), site font-gallery cards.

Review round 1 (Codex): a caught error inside BuildChar left the font and
scratch dictionaries on the dictionary stack. BuildChar now runs its body under
`stopped`, closes both, and raises the error again (`stop`; pscat and gs both
still report the original error). `pf-variant` checks each dial when the
variant is made: a wrong type is a `typecheck`, a value out of range a
`rangecheck` (`/DotShape` `/square` or `/round`, `/DotSize` in (0, 4],
`/ScanGap` in [0, 1), `/Interpolate` a boolean), and nothing is defined.
