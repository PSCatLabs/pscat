# #423: ribbon ink a little lighter, and strike weight less even

Follow-up to #212 (ribbon ink) and #215 (the blind comparison noted "the
render's type is slightly darker and more even"). Design in
`docs/dev/design/TYPEWRITER.md` ("Tuned in #423"). Library change only:
`lib/typewriter.ps` (`tInkStrike`, `tStrikeGlyph`, the defaults and presets).

## Measured, before and after

Scratch scripts (not committed) compared scan crops with renders from
`ink_compare.py`'s own panels, at the scan's resolution. A pixel is ink below
128 after the paper is normalised to 255. Numbers are means over seeds 1-6
(coverage, darkness) and 8 seeds x 30 `e`s (strike weight):

| | scans | before | after |
|---|---|---|---|
| T2 coverage of the text band | 0.154 | 0.178 (x1.16) | 0.151 (x0.98) |
| T4 coverage of the text band | 0.195 | 0.253 (x1.30) | 0.206 (x1.06) |
| darkness of ink pixels, T2 / T4 | 0.775 / 0.770 | 0.804 / 0.794 | 0.779 / 0.762 |
| `e` ink mass, sd / mean, `/manual` | T7: 0.205 (0.12 without the blank `e`) | 0.039 | 0.104 |
| same, `/electric` (O'Brien-like control) | | 0.024 | 0.026 |
| `e`s below 0.75 of the mean mass, `/manual` | T7: 1 in 14 | 0 | 5% |

The T7 scan figure uses tiles that include neighbour strokes, so it is an
upper bound. The pressure was already drawn with sd 0.08, but the ink did not
follow it: a hole's area is only a third of its level, and the slope was 0.55
per unit of pressure, so the mass moved about 4% per sd.

## What changed

- Body stroke 0.3 -> 0.2 of the spread, feather 0.85 -> 0.45. The spread
  (`inkW`) is untouched, so `/InkModel false` is the same ink as before.
- Base hole level 0.1 -> 0.15 (0.21 at the default ribbon and mean pressure).
- Pressure slope on the hole level: 0.55 -> 3 below 1, 0.15 -> 1 above.
- Pressure sd 0.08 -> 0.12 on `/manual` and `/worn` (default too). `/electric`
  stays at 0.03. This also widens flat ink's pressure (spread width) at
  `/Wear > 0`.
- The very-light strike was a fixed -0.5 at 1%. Now its probability is
  sd / 6 (2% at 0.12, 0.5% on `/electric`) and its depth is drawn from 0.3 to
  0.7 (strike slot 8, unused before). The strike's draws are uni 4, 5 and 8
  and gau 1, 2, 3 and 6 (slots 5-7, 9-11, 13-15, 25-27); uni 5 shares slot 5
  with gau 1, as before.
- Copies: the toner spread factor 0.7 -> 1.1, since the thinner feather made
  `/Copies 3` less fat (the released test expects 10% more dark pixels).

Per-key means, measured on `/manual`, machine 1, 8 seeds x 24 strikes: the
ink mass is 0.79-0.86 of the old mass on every key tried. The little-finger
keys (`a q z p ;`) lost 0.78-0.83, against 0.86 for `e` and `o`: the steeper
slope also scales the per-key terms (finger, `mWt`), a few percent more on the
light keys, which is the direction the scans show. Not split into two slopes.

## Tests

- `the_weave_opens_voids_and_a_dry_ribbon_opens_more` now measures voids
  against the bare face (`/Ribbon 0`, flat), not against flat ink at the same
  ribbon. The thinner edge no longer counts as a void.
- `tests/released.rs`: the "is off" checks use `differing_visibly` (more than
  2 grey levels). The typed page through `rel-page` and through `exec` differs
  by 17 pixels of one grey level at one glyph (a clip edge at another device
  origin), as #212 saw. Not a bug in either library.

## Renders redone

Found with `scripts/identity_check.sh --loader lib/typewriter.ps` (3 files,
all differ; the control, the specimen, differs as it must), plus the output of
`scripts/typewrite.sh`: `docs/dev/design/typewriter_specimen*.png`,
`typewriter_memo*.png`, `released_specimen*.png`,
`gallery/renders/released_memo.png`, and the composites `ink_vs_real.png`,
`memo_vs_real.png`, `blind_grid.png`, `release_vs_real.png`. The gallery
manifest text is still right (`gallery.py check` passes). The blind grid's key
is in the PR, not here.
The author's reading is not blind (the author has seen every scan): the
renders now look lighter and less regular, and still a little more regular than
the scans.

## Not done

- A light strike dots out; the scans' light strikes are thinner and greyer.
  Filed as a p3 issue.
- Elite stays about 6% heavier than T4 by coverage; the face itself.
