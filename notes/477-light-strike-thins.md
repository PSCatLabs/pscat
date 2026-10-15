# #477: a light strike thins, not only dots out

Follow-up to #423 (`notes/423-ink-lighter-uneven.md`). Design in
`docs/dev/design/TYPEWRITER.md` ("Tuned in #477"). Library change only:
`lib/typewriter.ps` (`tInkStrike`, `tBody`, new `tErode` and constants).

## The problem

After #423 a light strike lost its ink through the weave alone (the hole level
rose by 3 per unit of pressure shortfall), so it read as a screen of pin-holes
on a full-width stroke. The scans' light strikes (T2, T7) are thinner and
greyer with their edges intact.

## What changed

- `tErode` clips the body to the glyph shifted by `+-e` in x and y (four
  clips, intersected: an erosion by a square of half-width `e`). A clip, not
  paint, so the ink only darkens what is under it; a light strike over an
  earlier strike (overtype, underscore, `/Haze`) cannot bleach it. It is applied
  inside `tBody`, so both passes of a one-side-heavy key get it.
- `e = 60 x (1 - pressure - 0.08)` glyph units (0 below that, 20 at most). A
  strike at pressure 0.76 loses about 10 units a side of a 70-unit stem; a
  very light one (0.3 to 0.7) is a hairline. No erosion at mean pressure, as the
  scans' normal strikes have crisp dark edges.
- The hole level's slope below pressure 1: 3 -> 0.6, and 0.1 beyond a 0.15
  shortfall. The ink a light strike loses now goes to thinning.
- The feather's width falls with `e` (by 60% at the cap).
- A one-side-heavy key's light side: its extra sparseness 0.5 -> 0.3 of the
  side, and it is eroded 16 x the side more.
- A photocopy (`/Copies > 0`) is not eroded: the toner fills a thin stroke in.
- `/InkModel false` is unchanged. Elite is not given a base erosion (it would
  change every normal strike, which the scans do not support).

## Measured, before and after

Scratch script (not committed): `/manual` at 300 dpi in pscat, seeds 1-8,
the text band's coverage (pixels below 128 after the paper is normalised),
integrated mass (mean of 1 - luminance over the band) and the darkness of
the ink pixels. Mass is reported because coverage alone can be gamed by greying
an edge. The `e` figures are 8 seeds x 30 `e`s, `/manual` machine 1.

| | before | after |
|---|---|---|
| T2 text (pica, machine 1) coverage / mass / darkness | 0.152 / 0.133 / 0.777 | 0.145 / 0.131 / 0.792 |
| T4 text (elite, machine 4) coverage / mass / darkness | 0.165 / 0.145 / 0.767 | 0.156 / 0.143 / 0.785 |
| `e` ink mass, sd / mean (scans: T7 0.12 to 0.205) | 0.104 | 0.168 |
| `e`s below 0.75 of the mean mass | 4.6% | 5.8% |
| mean `e` mass | 271 | 267 |

The ink is nearly as heavy overall (mass -1.5%, coverage -5%) but its weight
varies more, closer to T7 (the `e` sd/mean 0.104 -> 0.168). The darkness of an
ink pixel rose a little (0.777 -> 0.792; the scans' 0.775), because the
speckled light strikes that used to pull it down are now thin and solid. The
elite excess against T4 was about 6% by coverage (#423); it is about 5% lower
now, but pica (T2) is also about 5% lower, since both faces share the erosion
(T2 was x0.98 of the scans before, so about x0.93 now).

Reading `ink_vs_real.png` (pscat and gs rows): the light `l` and `tt` of T2 and
the light `e` of "the" in T10 are thin grey strokes with intact edges where
they were a dotted screen; T7's light `e` looks like the scans' faint tiles.
gs still shows some dotting on a light strike at 300 dpi (its clip edges are
not antialiased as pscat's); it is much less than before.

## Renders redone

`scripts/identity_check.sh --loader lib/typewriter.ps --loader lib/released.ps`
(control: a driver that types a light strike): the same three files as #423
differ and nothing else, `examples/typewriter_specimen.ps`,
`examples/released_specimen.ps` and `gallery/released_memo.ps`. Regenerated
with their committed commands (they reproduce byte for byte from the base tree):
`docs/dev/design/typewriter_specimen*.png`, `typewriter_memo*.png` (via
`scripts/typewrite.sh`, command in `docs/guides/fonts.md`),
`released_specimen*.png`, `gallery/renders/released_memo.png`; and the composites
`ink_vs_real.png`, `release_vs_real.png`, `memo_vs_real.png`, `blind_grid.png`
by their tools. The manifest text is still right. The blind grid's key is in the
PR, not here.

## Not done

- Pica and elite share one erosion; a per-face weight would separate T2 from T4.
- gs still dots a light strike's feather and body a little at 300 dpi.
