# Dry media: chalk, crayon, charcoal and board math

These libraries draw the way a dry stick does: pigment lands only where the stick
meets the raised tooth of a surface (slate, newsprint, drawing paper), so strokes
break into streaks and grain. They are all built on one core, `drykit`, and
share its canvas with the [painting](painting.md) tools. Load artkit, paintkit
and drykit first (each example below shows the lines).

## The core: drykit

`lib/drykit.ps` (design notes in `docs/DRYMEDIA.md`) is the core of
the dry media: chalk, crayon and charcoal. It draws the current path as a
dry stick dragged across a toothed surface, and pigment lands only where
the stick's contact patch meets raised tooth.
- **`dksurface`** names the surface: slate, green board, newsprint, drawing
  paper or construction paper. Each is a `pkcanvas` tooth plus the
  surface's colour, so paintkit and drykit marks share one canvas.
- **`dkstroke`** draws in the current colour.
  - Light `/Touch` catches only the tooth's peaks. The knuckles are stretched
    along the direction of travel, so the gaps read as streaks.
  - `/Tilt` lays the stick on its side.
  - `/Wear` flattens the tip along a stroke.
  - `/Lanes`, `/Load` and `/Dropout` break it into striations.
  - `/Fallout` scatters loose particles beside it.
  - Tone builds in deposit levels, drawn faint-first.
- **`{ ... } dkbatch`** draws several `dkstroke` calls level-major, so
  crossing hatch layers stay dark where they cross.
- **`{ ... } dkclip`** clips to the current path while the procedure
  runs, and the strokes it makes keep that region inside a batch. So
  differently clipped strokes, such as overlapping hatched regions, can
  share one batch. A plain `clip` (or `eoclip`, `rectclip`) made inside
  the batch is kept the same way: `{ gsave region clip ... grestore }`
  works, nested and even-odd clips included. One catch: a procedure
  bound before the batch calls `clip`, `gsave` and `grestore` directly,
  so drykit sees neither its clips nor its restores. If you pass a
  callback that clips and strokes to a bound `gsave ... exec ... grestore`
  wrapper, the batch's later strokes stay under that clip. Use `dkclip`
  inside such callbacks.
- **`/After true`** on a `dkstroke` inside a batch draws that stroke on
  its own after the batch has rendered, in call order, in its own colour.
  It is how an eraser or a smudge can be drawn from inside a batch, over
  everything the batch lays.

drykit takes no draws from the caller's `rand` stream. Its surface and
tool-side randomness come from private seeded generators, and fallout goes
through a seeded `scatter`, which restores the stream. Adding a stroke
never moves another mark.
Fine tooth relies on Ghostscript's fill adjust being off, and drykit turns
it off itself. `examples/drykit_specimen.ps` shows one stick on all five
surfaces, at three pressures, at three tilts, with a wearing tip and with a
crossing.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
<< /Surface /greenboard >> dksurface
0.93 0.94 0.90 setrgbcolor
newpath 40 60 moveto 300 80 lineto
<< /Width 3 /Touch 0.7 >> dkstroke
```

## Chalk on a board: drychalk

`lib/drychalk.ps` is chalk on a board, built on drykit.
- **`dcboard`** (`x y w h opts`) dresses a board and makes it the surface
  for chalk. It draws:
  - the board's tone, black slate or green board;
  - a dust film that is never flat;
  - the last lecture's writing, erased (`/Ghost`, a path procedure);
  - eraser swipe arcs, darker finger wipes and lighter smudges;
  - dust settled along the ledge (`/Ledge`) and in the chalk tray
    (`/Tray`).
- **`dcchalk`** strokes the current path as chalk, one stroke per subpath.
  - Each stroke lands heavy, tires in the middle and lifts thin.
  - Its weight differs a little from its neighbours'.
  - Dots are pressed, and dust falls beside every stroke.
  - With `/Tilt`, the stick lies on its side for broad underlines and
    shading: two bright edges over a granular face.
- **`/name dcchalkcolor`** sets white or a soft yellow, pink, blue, green
  or orange.

The defaults were tuned against the photographs in `docs/drymedia/refs/`.
Real chalk reads about halfway from the board to white, not white.
`examples/drychalk_specimen.ps` shows `gallery/hundred_lines.ps`'s
pen-stroke chalk before and after, on both boards.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/drychalk.ps) run
40 40 520 300 << /Surface /slate /Ghost { (old notes) 60 250 << /Size 30 >> sgpath } >> dcboard
/white dcchalkcolor
newpath (fresh notes) 60 180 << /Size 32 >> sgpath << /Width 1.8 >> dcchalk
```

## Crayon: drycrayon

`lib/drycrayon.ps` is wax crayon on paper, and a child's ways of drawing with it,
built on drykit. `dypaper` sets up the paper (drykit's coarse construction-paper
tooth on a white ground), `/name dycolor` picks a colour from the crayon box,
and `dycrayon` draws the current path as wax. The "child's drawing" motions
each build a path and paint nothing, so they compose with `dycrayon`:
`dywobble` (a shaky, overshooting version of a path), `dyscribble`
(back-and-forth scribble fill over a region), `dyloop` (a circle drawn in one
loop that doesn't close), `dyfigure` (a stick or tadpole figure), `dyrays` (quick
sun rays around a disc) and `dygrass` (quick grass blades along a path). They
draw only from artkit's `frnd`, so they are deterministic under your own
`srand`, and `--sweep-seed` varies the composition while the paper texture stays
put. `examples/drycrayon_specimen.ps` shows the medium and
`examples/drycrayon_motion_demo.ps` the motions; the gallery piece *My Family
and Our House* is a whole picture drawn this way.

## Charcoal: drycharcoal

`lib/drycharcoal.ps` is charcoal on drykit, with three media (`/vine`:
grey and dusty; `/compressed`: dark and dense; `/pencil`: a sharp point) and a
`/Grade` from 0 (hard) to 1 (soft). Its words are call patterns for
`dkstroke`: `dhstroke` draws the current path in a medium, `dhhatch` hatches a
region to a value (a number, or a `{x y -> value}` callback), `dhsmudge` is a
stump's or finger's track, `dhlift` is an eraser's track (kneaded or hard), and
`dhsketch` lays faint construction lines. It needs hatchkit loaded.
`dhsmudge` and `dhlift` may run inside a `dkbatch`: they are drawn after the
batch's strokes, in call order, over all of them (even hatching made later
in the batch), so to hatch over a lift, hatch in a later batch.
`examples/drycharcoal_specimen.ps` is the specimen; the gallery piece *Study
after Berger, 1864* is a charcoal portrait built with it.

## Single-stroke glyphs: strokeglyphs

`lib/strokeglyphs.ps` gives single-stroke skeleton glyphs for board math as *paths*, not
ink: capitals, handscript's lowercase and digits, a math hand (curly x,
crossed z), Greek, operators and relations, and growing delimiters.
`sgpath` appends a string's jittered centerlines to the current path
(UTF-8 or an array of glyph names), `sgwidth` measures from the fixed
advances, `sggrow` builds a sum, product, integral, radical, paren,
bracket, brace or bar sized to a height and depth, and `sgmetrics` gives
the advance, box, italic correction and TeX math class. Jitter comes from
its own seeded generator, never the caller's `rand`, and
`examples/strokeglyphs.ps` is the specimen sheet. A character or glyph name
with no glyph raises `sg-missing-glyph:<char>` from `sgpath` and `sgwidth`
before anything is drawn; `/Strict false` restores the old
advance-300-and-draw-nothing behaviour. It also has the board-math
symbols a lecture needs: `≡ ∼ ≃ ⊆ ⊃ ⊇ ↦ ⊥ ∝ ∴ ∘ ∗ ⋆ ∖ ⟨ ⟩ ‖ ℏ ℓ ι υ Υ`, the
double, triple and contour integrals `∬ ∭ ∮` (also `sggrow` symbols), and
superscript digits (`i²`, `x⁻¹`); `examples/strokeglyphs_board_math.ps`
is their specimen.
It has subscript digits, signs and letters (`H₂O`, `xₙ`, `Σᵢ₌₀`,
derived from the digit skeletons the way the superscripts are) and the
variant Greek letters `ϑ ϰ ϱ ϖ ς ϵ ϕ`; `examples/strokeglyphs_subscripts_greek.ps`
is their specimen. In mathkit `\varphi \vartheta \varkappa \varrho \varpi
\varsigma` draw the variants, and `\epsilon` is the lunate ϵ while
`\varepsilon` stays the three-shaped ε, as in TeX.

```postscript
(lib/handscript.ps) run
(lib/strokeglyphs.ps) run
newpath (∀ε>0 ∃δ>0) 40 300 << /Size 40 /Seed 7 >> sgpath
1.5 setlinewidth 1 setlinecap 1 setlinejoin stroke
```

## TeX math as chalk strokes: mathkit

`lib/mathkit.ps` lays out a subset of TeX math as those
skeletons with TeX's box model. The subset covers:
- scripts, `\frac`, `\sqrt[n]{}`;
- `\sum`, `\prod`, `\int`, `\iint`, `\iiint` and `\oint` with limits, and `\lim`;
- `\left( \right)` delimiters that grow to fit, including `\langle \rangle`
  and `\|`, or the characters `⟨ ⟩ ‖ ∥` typed directly;
- relations and operators such as `\equiv \sim \subseteq \mapsto \perp
  \therefore \circ \ast`, and `\hbar \ell`;
- `*` is `\ast`, as in TeX: a binary operator between terms, and `z^*` or
  `A_*` a raised or lowered asterisk for a conjugate or an adjoint;
  inside `\text{}`, `\mathrm{}` and `\operatorname{}` it is a raised text
  asterisk (strokeglyphs' `/asterisk`), as TeX sets it in text;
- matrices and `cases`, accents and `\boxed`;
- Greek and operator names;
- `\overbrace{...}^{label}` and `\underbrace{...}_{label}` with a grown
  brace, `\overset`, `\underset`, `\stackrel` and `\substack` limits;
- `\displaystyle`, `\textstyle` and `\scriptstyle` mid-formula;
- `\not` before a relation (`\not=` is ≠, `\not\in` is ∉, anything else
  gets a slash), `\bmod` and `\pmod{n}`.

Anything outside the subset fails with an error that names it. The
layout is written with a lecturer's irregularity: sizes and gaps vary,
scripts sit a little high or low, fraction bars overshoot, and the
baseline slopes and drifts (`/Irregular`, where 0 is clean TeX).
- `mkpath` appends a formula's path and `mkmetrics` measures it.
- `mkalign` writes a derivation aligned on `=`. It returns a record per
  line, which `mkframe` and `mkunderline` follow along its slope. With
  several `&` on a line it sets TeX align's columns (right, left, right,
  left ...), each pair `/ColumnGap` (in multiples of `/Size`, default 2)
  after the last.
- `mkarrow` points a side note at a term.

Every TeX backslash is written twice in a PostScript string.
`examples/mathkit_specimen.ps` is the specimen: the fixed test set, from
the quadratic formula to Maxwell's equations, and a derivation in chalk.
`examples/mathkit_structures.ps` shows the structures issue #340 added
(`docs/drymedia/mathkit_structures.png`).
The gallery piece `gallery/chalkboard_lecture.ps` fills a
whole board with it: Euler's formula derived across three drychalk
panels.

```postscript
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/mathkit.ps) run
newpath (\\int_{-\\infty}^{\\infty} e^{-x^2}\\,dx = \\sqrt{\\pi}) 40 300
  << /Size 36 /Seed 3 >> mkpath
1.6 setlinewidth 1 setlinecap 1 setlinejoin stroke
```
