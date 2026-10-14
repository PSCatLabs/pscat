## Alpine Lake: shadow planes laid with pktrowelplane (issue #361, 2026-10-14)

The shadow face of each peak in `gallery/alpine_lake.ps` (#118) was the hand-built pattern
`pktrowelplane` (#180) turns into a procedure: a triangle hung off the right ridge, a `clip`, and a
`for` loop of vertical `pktrowel` pulls 7pt apart, each with a small `lkj` wobble at its far end.
#180 left the swap out because it moves the committed render. It is done here.

**What changed.** The `gsave ... clip ... for ... grestore` block in `lkrange` is one
`pktrowelplane` call, still checkpoint 2 of its peak (`lkdo`, so the reflection replays it exactly
as before). The plane keeps the old blade (`/Width 12 /Angle 12 /Coverage 0.85 /Viscosity 0.7
/Scrape 0.35 /Drag 0.5 /EdgeBuildup 0.15 /ColorJitter 0.03`), runs `/Direction -90`, and takes
`/Overlap 0.34`, which is the old 7pt pitch for that blade (band 12 * cos 12 * (0.35 + 0.65 * 0.85)
= 10.6pt, 1 - 7/10.6). The hand-made clip is gone because the plane clips to its own path, and
`lkknife0`, which only the loop used, is gone with it. `lkj`'s per-pull wobble is replaced by
the plane's own `/AngleJitter` and `/RowJitter` defaults.

**Look.** Compared at 2x against a fresh render of `main` (which matched the committed still byte
for byte). The planes read the same: the same crisp ridge edge, the same blade streaks. The
rows are a little more even, with fewer bare bright vertical bars where the hand loop's random
wobble let two pulls drift apart. `/Overlap 0.5` (the cookbook recipe's value) was also rendered:
a denser, smoother plane, not better enough to leave the old pitch.

**The seed stream moved, and so did everything drawn after it in each range.** The plane draws
a different number of values than the loop, and each range reseeds only at its top, so the snow
pulls and foot mist of both ranges (and their reflections) are re-rolled: the picture is the
same scene with different marks. Every other scene element seeds itself, so nothing outside the
two ranges can move through the shared stream. A pixel diff of the old and new 2x stills agrees:
rows 0 to 143 (the sky above the peaks), 684 to 798 (apart from one row) and 1052 down (the
foreground bank) are identical; the changes sit in the ranges and their reflection (and
wherever their mist shows through the far forest's crest). The reflection was checked by
flipping a near peak's mirror image against it: the shadow planes and their streaks line up.

Ghostscript (`gs -dNOSAFER`) still renders it without error.

## Not done

- `gallery/README.md`'s Alpine Lake entry still says "`pktrowel` snow and shadow planes clipped to
  each ridge"; the gallery manifest work (#251) is rewriting that file, so it is left for it (https://github.com/jeromebanks/postscript_interpreter/issues/435).
