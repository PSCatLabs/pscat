## A copy erodes its thin strokes by the glyph outline; `/Dropout` retired (issue #547, 2026-10-15)

Follows #449 (`notes/449-thin-stroke-dropout.md`, `rel-page /Dropout`, a hole
field standing in for erosion because a page cannot read a stroke's width) and
#477 (a light strike's `tE`). T15
(`docs/dev/design/typewriter/refs/T15_jfk_release_copy.jpg`) is a release copy
whose thin horizontals are gone and whose stems stay. Comparison:
`notes/547-compare.png` (T15, then generation 3 and generation 1 before and after).

## What shipped (`lib/typewriter.ps`)

On a copy (`tCopyC` above 0) `tInkStrike` draws the feather and the body inside a
clip (`tGrowClip`): the glyph grown by the spread's reach, then shrunk by `tEy`
in y only (the grown glyph shifted up by `tEy` and down by `tEy`, intersected).
A stroke thinner than about 2 x `tEy` goes, a thicker horizontal thins, and a
vertical stem keeps its width. The counter fill (`tClogBand`) stays outside the
clip. An original is not touched: `tEy` is 0, no `gsave` runs, and the code path is
the old one.

- `tEy = tECopy x tCopyC x clamp(pBody, 0, 1)`, `tECopy` 45 glyph units. `pBody`
  is the strike's hole level (pressure, ribbon, slow page wear, the copy's
  contrast curve), so a light strike loses its horizontals and a heavy one only
  thins, and a word loses them and the next keeps them. `tCopyC` is 0.45, 0.70, 0.83
  for one, two, three copies.
- The clip grows by `tEr = (inkW + fW) / 2` (the feather's reach, 0.225 of the
  spread) in x and by `tEryK` = 0.5 of it in y; the body is `2 tEr` wide on
  a copy where the clip runs (it was 0.2 of the spread), so the stems stay fat.
  Without that a copy was lighter than the original it was copied from.
- `rel-page /Dropout`, `rlDropClip` and the redaction-bar replay (`rlBarsAgain`,
  `rlRecOn`) are gone, as the owner's comment on #547 suggested. A caller that
  passes `/Dropout` now gets `rel-unknown-option`; nothing in the repo did.

## Why y only, not `tErode`

The face (Courier Prime, `lib/typewriter_outlines.ps`) is near monoline: the
ridge of a distance transform of a large render has 1449 of 1540 points at
70 to 80 glyph units, the E arms 65 against the stems' 70. There is no thin
stroke to find. `tErode` is a square, so it would thin the stems by 2 x `tE` and kill
the diagonals (a bowl at 45 degrees is `sqrt 2` thinner to a square) first, the
opposite of T15, where the thin parts are horizontal. Eroding in y keeps every
stem whole and takes the arms of `e` and `E`, the tops and bottoms of bowls, the
serifs and the underscores.

## Why a nine-shift union and not `strokepath`

The clip needs (glyph grown by the spread), as one path. Growing by `strokepath`
and appending the glyph works in pscat but not in gs: gs's stroke outline winds
the other way in places, so the nonzero fill has holes (checked on `e a o B 8 g @ & R`).
Nine copies of the glyph shifted by 0 and by the eight points round a circle of
radius `tEr`, drawn into one path, are a union under nonzero because every copy
winds the same way (a translation keeps the winding). The features are 65 or
more wide against a reach of 6 to 14, so the shifts leave no gaps. `clippath` is
no help either: pscat's returns only the newest link of the clip chain.

## Tuning

Sweep on the specimen's third sheet at 300 dpi (`tECopy`; y reach; body):
70 broke words ("Vis:i!or"); 55 lost "the" to "t.he"; 40 to 50 read well
(legible, bars gone from light strikes). Without a full-reach body the copy had
less ink than the original. The ink of the typed test line (72 dpi, dark pixels
under 128 / ink mass): original 259 / 297; a copy before 418 / 442; after 266 / 267.
This is round 1's weight; round 2 below restores it.

Codex's review of the PR found the growth radii were applied to the wrong axes
(x got `tEryK` of the reach and y the whole of it); the first sweeps above ran
on that. Fixed, and the output compared again at 300 dpi: slightly stronger
horizontal erosion, the same legibility, `tECopy` kept at 45.

## Cost

CPU (user + sys, `/usr/bin/time -l`, one binary, two trees): the specimen at 150
dpi 6.56 s before, 6.72 s after (+2%: the hole field is gone, the clip is added);
the gallery memo at 144 dpi 3.31 s before, 3.61 s after (+9%, a first copy). Guard only.

## Tests

`tests/typewriter.rs`: `a_copy_loses_its_thin_horizontals_and_keeps_its_stems`
(sixteen Es: arm ink 357,170 to 261,412 without and with erosion, stems 265,887 to
252,714, with the negative control `TWState /tECopy 0 put`; a first copy loses less than
three), `the_erosion_leaves_an_original_alone`. `tests/released.rs`: the three tests of
`/Dropout`, the bar-replay and recorder tests went with it; `a_copy_whitens_the_paper_and_the_type_gets_fatter`
now compares the spread with `/tECopy 0` and checks the erosion takes ink back out;
its rel-page against by-hand ink tolerance is 30% (it was 12%): the eroded strokes are
thin, so a sub-pixel shift of the copy moves their dark pixels.

## Renders that changed

`scripts/identity_check.sh --loader lib/typewriter.ps --loader lib/released.ps`:
`gallery/released_memo.ps`, `examples/released_pages.ps` (both pages) and
`examples/released_specimen.ps` differ; `examples/typewriter_specimen.ps` is identical,
and the specimen's generation 0 sheet (the left third) has 0 differing pixels at 300 dpi.
Regenerated: `gallery/renders/released_memo.png` (144 dpi),
`docs/dev/design/released_specimen.png` (150 dpi) and `_gs.png`,
`released_pages_1.png` and `_2.png` (110 dpi: the committed ones are byte-identical to a
110 dpi render of the base), `typewriter/release_vs_real.png`, `memo_vs_real.png` and
`blind_grid.png`, and the new `notes/547-compare.png`.

## Not done

- `/InkModel false` (the flat ink) has no erosion on a copy; its fat strokes are #211's.
- Weight against T15 and the underscore dashes (#592) are answered in round 2.

## Round 2 (review of PR #594; absorbs #592)

The first version scaled the erosion by the strike's hole level up to 37 glyph
units, and the user rejected the look: words next to each other were very
different (`thir`, `visit`, `signs` nearly gone beside `book`, `floor`, `on`), the
page was lighter than both the original and T15 (heavy and even), and the
underscore rule broke into dashes (an underscore is 66 thick, as thick as a
letter's bars, so a strike with `tEy` above 33 removed it). Changes:

- `tEy` is capped at `tECopyCap` = 18. Nothing vanishes (2 x 18 is under the 79 the
  clip leaves a bar), the bars thin, and what is under about 36 thick (serifs,
  joins) breaks. A first copy's `tEy` is at most 20, so it is as it was.
- The reach grows from the second copy on: `tEr` is multiplied by
  `1 + tEgrow x (tCopyC - 0.45)` (1.6; 1 for a first copy, 1.56 for a third), so the
  third copy's stems and bars are fatter.
- A name clash found on the way: a first attempt called the cap `tECap`, which is
  the light-strike cap of #477 and was in use; the copy cap is `tECopyCap`.

Typed test line at 300 dpi (dark pixels under 128 / ink mass): original 37,701 /
33,263; first copy 38,154 / 32,544 (round 1: 37,635 / 32,173); third copy 44,616 /
37,651 (round 1: 35,164 / 30,021; no erosion at all: 47,889 / 40,702). The specimen's
third sheet by ink mass: before #547 471,868, round 1 441,481, now 469,024; its first
sheet's copy differs from round 1 in 3,426 pixels (over 30 grey levels) and has the same
mass. Sixteen Es: arm ink 357,170 to 316,678 with the erosion, stems 265,887 to 280,444.
The weakest of sixteen cells keeps 0.84 of its ink (0.58 with no cap); an underscore
rule has 99% of its columns inked (20% with no cap at `tEy` 37).

Tests: `a_third_copy_is_as_heavy_as_its_original_and_no_letter_fades` and
`an_underscore_rule_stays_continuous_on_a_third_copy` (each with a negative control
that lifts the cap), `a_copy_loses_its_thin_horizontals_and_keeps_its_stems` relaxed.
`gallery/time_traveler_memo.ps` (#591, an original) is byte-identical.
