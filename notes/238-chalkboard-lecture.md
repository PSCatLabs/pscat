## Lecture 14: the chalk gallery piece (issue #238, 2026-10-14)

`gallery/chalkboard_lecture.ps` is a slate board as a professor left it after
deriving Euler's formula from the Taylor series. It is the chalk track's
gallery piece for epic #232. It uses no new library code: the formulas come
from mathkit (#237), and the chalk and board from drychalk (#235).

**The board, left to right:**

- **Left panel.** The date, and the title with a double underline drawn as
  two separate strokes (C8). Below it, "Recall:" and the Taylor series of e^x,
  cos and sin, with "even" and "odd" in blue. Last comes a small table of the
  powers of i.
- **Middle panel.** x = iθ is substituted. A pink "i² = -1 !" points at the
  sign that flips. The series is regrouped into two bracketed series, which
  are underlined and labelled cos θ and sin θ in blue. This stands in for
  `\underbrace`, which is #340. Then comes ∴ (three pressed dots, because
  mathkit has no `\therefore`), and e^{iθ} = cos θ + i sin θ boxed twice in
  yellow, the outer box looser.
- **Right panel.** A freehand unit circle in the complex plane, with axes,
  the point e^{iθ}, dashed projections and the angle arc. Below it, θ = π,
  e^{iπ} + 1 = 0 circled in yellow, "!!", and the homework boxed lightly in
  the corner.
- **Each panel is its own `dcboard`.** The panels differ in seed, haze and
  swipe count. Each carries a `/Ghost` from the last lecture (the Gaussian
  integral, `\sqrt{\pi}` and a sketched bell curve), erased at `/Erased 0.5`.
  The tray holds two felt erasers and chalk stubs, over `/Tray` dust.
- **Wear across the lecture.** `/Slant` grows from 2° to 4° to 6° panel by
  panel, and `/Irregular` from 1.3 to 1.6–1.7 to 1.9. `/Touch` falls from
  about 0.76 to about 0.6. The boxed and circled results are the exception:
  they are pressed at 0.82–0.86.

**Design calls and gotchas**

- **Draw each panel's chalk right after its own `dcboard`.** `dcboard` calls
  `dksurface`, so `dcchalk` takes the last board drawn as its surface.
  - The panels abut, and aluminium strips are painted over the joints
    afterwards.
  - No stroke crosses a joint, because the next panel's board would cover it.
  - Each panel's `/Tray` segment meets the next with no gap.
- **The piece's names live in `lectdict`.** That keeps gs's userdict lookups
  fast (notes/235), and every name carries an `lc` prefix. Two collisions
  happened before the prefix:
  - a helper named `loop`;
  - a variable named `lt`, which shadowed the operator for the rest of the
    page.
- **A `/Ghost` procedure passed through a helper must be `load`ed**
  (`/Ghost /bgh load`). Otherwise building the dict runs it.
- **`mkframe` takes `llx lly urx ury`,** not `x y w h`.
- **The formulas are placed by their records.** The labels beside the
  brackets read the `mkalign` records' `/BBox` and `/Origin`, so they follow
  the layout's irregularity rather than fixed coordinates.
- **Size against the reference numbers.** The stroke width is 1/16 to 1/18
  of cap height, inside C1–C7's range. The cap height is about 1/30 of the
  board, smaller than C7's 1/11 of a panel: a whole derivation has to fit on
  one board.

**Cost** (CPU `user`, release build, 144 dpi, a shared machine):

| | pscat | gs 10 |
|---|---|---|
| the 2x gallery still (`2 2 scale`, `--page 2880x1800`) | 7.2s | |
| `--dpi 144` / `-r144` | 7.2s | 3.4s |

This is well under DRYMEDIA.md 6.2's 30 s board budget. The piece lints
clean (`--lint`). It isn't in the CI lint list, so it adds no CI time.

**Comparison.** `docs/drymedia/lecture_compare_238.png`
(`prototypes/lecture_compare.py`) sets the still above C7, C1 and a new
public-domain reference, C11 (Fermi at the blackboard), with 1:1 details
beside C5, C1 and C3. **Where it falls short of the photos:**

- The real boards' writing is larger, brighter and rougher.
- Their lines are more crowded, and a hand varies its letter size more.
- The strokeglyphs "4" reads like ϟ at board size. That is the glyph data
  (#236), not the medium.

**Not sourced still:** a photo of a chalk box around a result. The box follows
`mkframe`.
