## drykit overlap run: thinning start and straight-line reach (issue #1483, 2026-11-27)

Two nitpicks from the review of #1478 (#1444). Both were measured against the
whole stroke before anything was changed. One is real and small (fixed); the
other is not a defect (documented). Every number is one binary, base lib against
head lib, 432 dpi, `/Width 8 /Touch { 0.4 mul 0.3 add } /Seed 3` on a green
board, worst level / pixels over 32 against the whole stroke.

**1. The catch is thinned from the tail's own first stop (fixed).**
`dkZthin` keeps every k-th stop of a run (k is the whole run's step, 2 here) for
the catch. The overlap run is only the tail of the earlier run, and it was thinned
from its own first stop, so when that stop was an odd index of the whole run the
kept stops were not the ones the whole stroke's catch keeps (the tail kept 7, 9,
10 where the whole run keeps 6, 8, 10). A catch stop's neighbours set the
direction of the knuckles near it, so on a curve the tail's catch differed from the
whole stroke's. On a straight line it cannot: the pieces were byte-identical
between the two thinnings (checked, the #1478 cases).

The case has to reach the code: `dkFover` fires with the tail starting at stop
j of the whole run, and the thinned stops differ only when `j mod k` is not 0.
Printed from `dkFover` and `dkZthin` for each case; a curve whose earlier run is
11 or 13 stops long (the curve ends at x = 73 to 79 or 91 to 97) has j = 7 or 9,
and one of 10 or 12 stops (x = 70, 82 to 88) has j = 6 or 8 and is unchanged.

Now the overlap's flags carry j (the 15th, index 14) and `dkZthin` keeps the whole
run's own stops (multiples of k from the run's start) inside the tail, plus the
tail's first and last. Only a run with flag 16 takes the new branch; every
other run, and every whole stroke, takes the old one.

Measured, 5 curve families (rising, falling, `/Levels 3`, `/Width 5`, touch
returning to a drawn band) x 5 end points x 46 cuts (0.30 to 0.75), pieces
after against pieces before:

- 151 of the 1150 cuts reach the changed code and change; the rest are
  byte-identical (the `/Width 5` family has none that differ; its tail starts on
  a stop of the whole run's lattice, or the overlap does not fire).
- The pieces move by at most 22 levels (36 and 51 on the returning-touch curve),
  on tens of pixels (up to 153 on two falling cuts).
- Against the whole stroke they get no worse at 148 of those cuts (the sum of the
  worst level over the changed cuts goes down in every family, for example
  1,508 to 1,391 on the rising one and 1,747 to 1,691 on the falling one, and
  pixels over 32 go from 356 to 349 and from 573 to 555). At the 3 cuts where the
  worst level rose (33 to 38, 36 to 37, 20 to 28) pixels over 32 are fewer or the same:
  seam noise.
- The worst single case, curve ending at x = 91, cut 0.50: 46 levels on 1 pixel over
  32, becomes 24 levels and none. Cut 0.52: 34/1 becomes 23/0. This is the pair
  `an_overlap_run_keeps_the_whole_runs_catch` pins (it fails on the old lib with 1
  pixel). Crops: `1483-drykit-overlap-nitpicks/curve-cut0.50.png` and `curve-cut0.52.png`
  (whole stroke, two pieces before, two pieces after; 3x of 432 dpi).

An ordinary `/From a` part has the same property (its catch is thinned from the
cut stop, not the whole run's lattice) and it is not small: see "Found while
measuring" below. Not touched here.

**2. "Within one reach" is a straight-line distance (documented, not changed).**
A path distance can only drop overlaps that the straight-line test lets through,
never add one. So the question is whether an overlap that the straight line lets
through, but the path does not (a band change a turn further along a tight curve),
costs anything. Tried: a tight curl (`40 25 2.5 0 540 arc`, 5 pt across under an 8 pt
stroke, so a band change half a turn on is 5 pt away by the straight line and 7.9
along the path, against a reach of 5.5) and four other shapes, cut every 0.02:

| case | cuts where the two tests differ | pixels over 32, straight (kept) | along the path |
|---|---|---|---|
| curl r 2.5, 540 deg, w 8 | 0.20, 0.22, 0.24, 0.26, 0.76, 0.78, 0.80 | 30, 33, 32, 28, 1026, 924, 841 | 42, 44, 43, 37, 1048, 937, 847 |
| curl r 3.5, 540 deg, w 8 | 0.42, 0.44, 0.78, 0.80 | 80, 92, 1397, 1339 | 111, 114, 1404, 1344 |
| U, w 4, 3 apart; w 6, 4 apart; loop; curl w 5 | 6 cuts | 130, 357, 388, 35, 43, 410 | 130, 362, 388, 37, 45, 409 |

(The large counts at 0.76 and later are the curl's own seam, a self-overlapping
stroke, present before and after.) So on every tight curve the straight-line test
joins the pieces as well or better than the path test, and the overlap it adds
where the path test would not is harmless (a few pixels' rim at most). That is
also what the geometry says: the shared stop's disc is near the cut in space, and
the whole stroke's layering there is by space, not by arc length. Kept as is,
and the comment in `dkFover` says so. Crop:
`tight-curl-cut0.22.png` (whole stroke, the pieces before and after this change,
which are byte-identical here, and the path test that was tried; 4x of 432 dpi).
`an_overlap_on_a_tight_curl_does_not_make_the_join_worse` bounds the pixels over
32 at 36 (30 and 32 today at cuts 0.20 and 0.24; the path test leaves 42 and 43, and
fails it).

**Found while measuring (not changed, proposed).** A constant-touch curve drawn in
two pieces (`/Width 8 /Touch 0.5`, a cubic) shows a seam of 77 levels on 186 to 326
pixels at cuts 0.15, 0.25, 0.35, 0.45, 0.60; no band change is involved (a constant
touch makes no overlap run). With the catch's thinning forced off (`dkZtk` 1) in
both the whole stroke and the pieces the same cuts are 33 to 53 levels on 1 to 16
pixels. So the same phase effect as nitpick 1, on every ordinary part: a cut part's
catch is thinned from its cut stop, not from the whole run's stops. The overlap
fix here only covers the overlap's tail.

**Limits of the lattice fix.** A `dkZsplit` half of an overlap run (only after a
budget split of a chunk) keeps the original `j`, so its second half thins on a stale
phase: the catch is still a superset, only the knuckle direction is off. And a whole
stroke long enough for `dkZgroup` to chunk thins per chunk (HANDOFF), so the
alignment matches the whole stroke only for runs that are not chunked.

**Identity.** The change is in the part branch only: a `/From 0` part and a whole
stroke never reach `dkFover` (it needs `dkFa` above 0). `scripts/identity_check.sh`
over the four drykit loaders: 13 of 13 pages identical, and the control (a curve in
two pieces at the band change, tail on an odd stop) differs between the trees as it
must. `/From 0 /To t` growth, 25 frames each, base lib against head lib with one
binary: `dkstroke` (rising touch, touch crossing bands, falling with `/Levels 3`),
`dcchalk /Tilt 75`, `dycrayon`, `dhstroke`, all byte-identical.
