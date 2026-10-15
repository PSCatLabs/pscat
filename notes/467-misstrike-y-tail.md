# #467: the mis-strike's vertical tail

Found while working #215. In `gallery/released_memo.ps` (Machine 11, Seed 1962,
`/Wear 1.3`) the `d` of "restricted" printed about 1.6pt above the line (it
read "restricte d"). `lib/typewriter.ps` gave 1% of strikes 4x the strike
offset on both axes.

## What the board says

`docs/dev/design/typewriter/REFERENCES.md`, Measurements (strikes off the
baseline by more than 0.5pt vertically): Duncan 6.6%, Szilard 0.8%, O'Brien
0.1%. Horizontally, more than 1pt: 0.8%, 0.9%, 0.2%.

The vertical numbers need no heavy tail. A normal of the measured per-strike
vertical sd (Szilard 0.19pt) gives 0.85% over 0.5pt; Duncan's sd of 0.29pt gives
8.5% (measured 6.6%); O'Brien's 0.16pt gives 0.18% (measured 0.1%). The
horizontal ones do need it: a normal of sd 0.2pt almost never lands 1pt off.
The board has no measured maximum on y, and OCR boxes are whole pixels (0.24 to
0.30pt), so "about 1pt" is a judgement from the crops, not a measurement.

## Change

One line: the mis-strike takes 4x on x (as before) and 2x on y (was 4x). The
draw slots are unchanged, so only the 1% of strikes that were mis-struck move.

## Measured (5320 strikes per run, 8 machines and seeds, 42,560 strikes)

The strike's vertical residual is `mY x wearM + sY` (the machine's bent slug plus
the strike), excluding the line layer, as REFERENCES' baseline fit excludes it.
In points, at `/Size 11.5`:

| /Wear | y multiplier | sd | over 0.5pt | over 1pt | max |
|---|---|---|---|---|---|
| 1 | 4 (before) | 0.173 | 0.56% | 0.07% | 1.55 |
| 1 | 3 | 0.169 | 0.43% | 0.02% | 1.19 |
| 1 | **2 (now)** | 0.166 | 0.27% | 0% | 0.84 |
| 1 | 1 | 0.164 | 0.19% | 0% | 0.68 |
| 1.3 | 4 (before) | 0.225 | 2.20% | 0.16% | 2.01 |
| 1.3 | **2 (now)** | 0.216 | 1.89% | 0% | 1.09 |
| 1.5 | 4 (before) | 0.259 | 4.43% | 0.25% | 2.32 |
| 1.5 | **2 (now)** | 0.249 | 4.14% | 0.03% | 1.26 |

At `/Wear 1` the fraction over 0.5pt (0.27%) sits inside the three machines'
0.1 to 0.8%, and at `/Wear 1.5` (Duncan's wear) it is 4.1% against Duncan's 6.6%
(it was 4.4%: the model was already under Duncan). The sd barely moves. The
farthest lift falls from about 2pt to about 1pt at the memo's `/Wear 1.3`.
A multiplier of 1 (no y mis-strike at all) is also inside the range, but the
issue and the line-up of x and y mis-strikes (a typist's raised letter is rarer
and smaller, not absent) argue for keeping some. 2.5 and 3 leave 1.3 to 1.5pt
lifts at `/Wear 1.3`. The horizontal side is untouched (over 1pt: 0.15% at
`/Wear 1`, board 0.2 to 0.9%).

The measurement was a throwaway: `tStrikeGlyph` printed the residual and the
mis-strike flag, run over about 5,300 strikes of a typed paragraph per machine
and seed, then reverted.

## Renders that changed

`scripts/identity_check.sh --loader lib/typewriter.ps --loader lib/released.ps`
(control: a probe key only the base has): exactly `examples/typewriter_specimen.ps`,
`examples/released_specimen.ps` and `gallery/released_memo.ps` differ, and
`scripts/typewrite.sh` for `examples/typewriter_memo.txt` (not in that corpus: its
render base equals the committed one, so it was compared directly). In the memo
the differences are 9 clusters of pixels, one per mis-struck letter (the memo
has 9 in 1158 strikes), and nothing else. Regenerated with a fresh release
binary, by their committed commands: `docs/dev/design/typewriter_specimen.png`,
`released_specimen.png`, `typewriter_memo.png` and their `_gs.png` (gs, only
as a check that placement agrees), `gallery/renders/released_memo.png` (144 dpi),
and by their scripts `ink_vs_real.png`, `release_vs_real.png`, `memo_vs_real.png`
and `blind_grid.png`. The blind grid's key was printed to stdout and is not in
the repo.

`notes/467-restricted-d.png` is the `d` before (top) and after (bottom), 6x;
`notes/467-memo-misstrikes.png` is all nine of the memo's mis-struck letters,
before above after.

The `d` in "restricted" now sits about 0.8pt high: still a raised letter, no
longer half an x-height.

## Not done

Nothing filed: no leftover came out of this.
