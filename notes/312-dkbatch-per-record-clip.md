# #312: a dkbatch holds strokes under different clips (`dkclip`)

## What shipped

- `proc dkclip -` (lib/drykit.ps, public). It clips to the current path the
  way `clip` does, while `proc` runs: `gsave clip newpath proc grestore
  newpath`. It also captures the region once, as a device-space path
  (`pathforall` under an identity CTM, into operator objects so a
  redefined `moveto` can't change it). The entry `[parent path]` is kept in
  `userdict /dkClip` while the procedure runs, and nested calls chain.
  The previous entry comes back on exit, and on an error too.
- In an open batch, `dkstroke` records the entry in force as the record's
  `/clip`. It records `null` when that entry is the one the batch was
  opened in (`dkBatch /base`), because that region is still in force when
  the batch renders, and clipping to it twice would thin its anti-aliased
  edges. Records drawn at once (no batch) always store `null`.
- At render time, `dkZsame` also requires the same `/clip` (by identity,
  checked only after touch, streak and colour already match), so records
  under different regions never share a catch. `dkZdraw` replays the
  record's links, up to the batch's base (`dkZbase`), before the catch.
  `dkZfallout` replays them once per record, around all its pieces.
- `dhhatch` (lib/drycharcoal.ps) joins a caller's batch through `dkclip`.
  This lifts `dhhatch-cannot-run-inside-a-dkbatch`. With no batch open it
  runs exactly the old code (`gsave clip { ... } dkbatch grestore`).
  `dhsmudge` and `dhlift` still refuse a batch: their `/Under` and colour
  handling assume they draw at once.

## Why not capture the plain clip (the issue's first proposal)

pscat's `clippath` (`Gfx::set_path_to_clip`) returns the newest clip
path when there is a single clip. Under nested clips it returns the rect
intersection when every link is an axis-aligned rect, and otherwise a
pixel trace of the mask. It also drops the eoclip rule, and pscat has no
`gstate` objects. So a generic per-stroke capture would not replay the
clip exactly. It would also cost a clip-path copy, plus a mask trace under
nested clips, for each of dhhatch's thousands of `dkstroke` calls. The
scoped call captures once per region, and identity comparison of the
entry keeps grouping cheap and exact. A plain `clip` inside a batch still
applies only if it is in force when the batch ends (documented in
`dkbatch`'s summary).

## Evidence

- Replay is faithful. A stroke with fallout, under an elliptical region in
  a rotated, scaled space, made inside `dkclip` in a batch, is
  byte-identical to the same stroke drawn at once under `gsave region clip
  ... grestore` (`dkclip_keeps_a_strokes_region_in_a_batch`). Controls:
  moving the region by 0.5 pt differs, and a plain `clip` inside a batch
  differs by more than 200 pixels. Skipping the fallout replay fails the
  test (checked by hand).
- Level-major across regions: `dkclip_regions_in_one_batch_stay_level_major`
  (0 dimmed core pixels in one batch; two batches dim more than 20).
  `overlapping_regions_in_one_batch_keep_crossings_dark`: overlap luma 119
  in one batch against 133 for two calls, and pixel-identical away from
  the overlap.
- Ghostscript: `ghostscript_keeps_a_dkclip_region_in_a_batch` (a middle
  third keeps 0.34 of the ink in pscat and 0.34 in gs). The specimen
  renders the same in gs.
- Identity. `scripts/identity_check.sh --loader` for drykit, drycharcoal,
  drychalk and drycrayon: 6 of 6 identical, and the control (the specimen
  driver) differs. The three gallery branches, rendered at 144 dpi with
  this branch's drykit and drycharcoal against main's, are byte-identical:
  `lincoln_charcoal.ps` (#343), `crayon_drawing.ps` (#336) and
  `chalkboard_lecture.ps` (#353). The specimen's left panel is
  byte-identical to main's render. On main its right panel raises
  `dhhatch-cannot-run-inside-a-dkbatch`.
- CPU, best of the budget tests' own runs, three rounds each, base → head:
  - drykit 100 strokes: 0.167–0.173 s → 0.169–0.171 s
  - drycharcoal hatch: 0.225–0.228 s → 0.226–0.228 s
  - drychalk board: 0.157 s → 0.155–0.158 s
  - drycrayon scene: 0.349–0.354 s → 0.351–0.353 s

  Flat: records outside `dkclip` pay one extra `eq` per matching pair.
  The two-region specimen in one batch costs the same as two calls (pscat
  0.79 s against 0.80 s at 144 dpi for the scratch version).

## Specimen

`docs/drymedia/dkclip_layering.png` (from
`docs/drymedia/prototypes/dkclip_layering.ps`): a disc and a square, each
hatched with `dhhatch` at `/Levels 2`. On the left they are two calls,
and the square's faint level greys the disc's crossings. On the right
they are the same calls in one `dkbatch`, and the crossings stay dark.
