# #532: Lecture 14, the final pass (2026-10-15)

`gallery/chalkboard_lecture.ps` (#238) is redrawn with the mathkit
structures from #340 and #439. It now draws on drykit's headroom from #324
and keeps the open 4 (#351) and the 7 fix (#298). The board, the ghost
lecture, the frame, the tray and the hand's wear across the panels are
unchanged. Only the formulas and their annotations changed. No library
changed.

## What the board says now

- **Left.** The e^x series has a second line, `1 + x + x²/2! + ⋯`, with
  `\underbrace`s under the first three terms labelled n=0, n=1 and n=2. That
  is the step a lecturer takes from the sum to the expanded series. cos and
  sin keep "even" and "odd". The powers of i become a two-pair `mkalign`
  (`i^2 = -1, i^3 = -i / i^4 = 1, i^5 = i`) with `/ColumnWander 1.5`. Under
  it is the rule `i^n = i^m if n ≡ m (mod 4)` (`\pmod`).
- **Middle.** The second line of the substitution is
  `&\overset{(*)}{=}`, and a pink "(*) i² = −1" beside the next heading
  answers the star. That replaces the old pink note and arrow. The regrouped
  brackets are each braced underneath in blue with `mkbrace`, labelled
  cos θ and sin θ. That replaces the hand-placed `mkunderline`s, which #238
  used as a stand-in for a brace. The ∴ and the double yellow box are as
  before.
- **Right.** The single "θ = π: e^{iπ} = −1" line becomes a two-pair table
  of the quarter turns: `e^{iπ/2} = i, e^{iπ} = −1 / e^{3iπ/2} = −i,
  e^{2iπ} = 1`, with `/ColumnWander 1.5`. It echoes the powers of i on the
  left, and leads to the circled e^{iπ} + 1 = 0.

## Design calls

- **The table runs from i² to i⁵, not from i⁰.** At board size, the
  superscript 1 in `i^1` reads as a prime (i′), and i′ is a different
  symbol on a maths board. The glyph is left alone, as the design intent
  asks, and the content changes instead. i⁵ = i also shows the period-4
  repeat right above the mod 4 rule.

- **`\overset{(*)}{=}`, not `\overset{i^2=-1}{=}`.** The latter was tried
  first. At chalk size the label's own = sits right over the relation's =,
  and the pair reads as ≡ ("i² ≡ ⁻¹"). A star with a footnote is what
  lecturers write, it reads at a glance, and the footnote can be pink. A
  label can't take its own colour, because the line is inked once.
- **`mkbrace` for cos/sin, not `\underbrace`.** An `\underbrace` label is
  inked with its line, so it would be white. The braces were blue before,
  and the colour helps the board read, so these are `mkbrace` annotations.
  `\underbrace` is used on the left, where white labels are right.
- **Placing an `mkbrace` inside an `mkalign` line.** The bracket's start is
  the line record's `/Origin` x, plus the `mkmetrics` width of the prefix
  (`&=`, `&\quad + i`) under that line's own seed (`/Seed` + 101 i), plus
  a small gap. Its end is the record's `/BBox` right edge. The brace is
  drawn left to right, so it points down.
- **Not used: `\not` and `\substack`.** Nothing on an Euler's-formula board
  calls for them. `\not=` draws a plain ≠, and a forced one would read as
  decoration. The issue lists the tools available, and these two had no
  natural place.
- **The hand is unchanged.** The seeds, `/Irregular`, `/Slant` and
  `/Touch` follow the old wear curve. No glyph was touched.
- **The helpers are now `bind def`.** The render is byte-identical with
  and without the binding.

## Evidence

- Renders: `pscat --page 1440x850 --dpi 144` (the 2x still). Before the
  change, the base tree reproduced the committed PNG byte for byte.
  `--lint` is clean.
- Ghostscript (`-dNOSAFER`, 72 dpi) renders the piece without error. The
  layout matches by eye. This is a correctness check only.
- drykit: the render completes with no error or refusal. Since #324 an
  overflowing catch is split silently, so that is what can be observed.
  #324 measured the lecture's largest catch at under 5% of `dkbudget`. This pass adds a few short
  strokes per panel, and the render stays well within DRYMEDIA 6.2's 30 s
  board budget.
- `lecture_compare_238.png` was rebuilt with
  `prototypes/lecture_compare.py` from the new still. Its crop boxes still
  frame the boxed result and the circle, because neither moved. Against
  the committed composite, the base tree's rebuild differed only in the
  header caption: #465 renamed the refs path in the script and never
  regenerated the image. So this rebuild also refreshes that caption.
  The boxed-result crop moved down 18 px, to `(1000, 1198, 1760, 1458)`,
  so that the cos/sin brace's "sin θ" label, which now sits just above
  the box, isn't cut in half along its top edge.
