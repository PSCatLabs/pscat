# #538: /BleedGap, and what /WornBand and /Bleed can claim

No scan to tune against: #468 found no public-domain two-colour or worn-lane
page and #551 (photograph our own) was closed as not planned. So this tunes
from #468's written evidence (`notes/468-typewriter-trait-references.md`,
`docs/dev/design/typewriter/REFERENCES.md`) and changes nothing by default.

## Added

- **`/BleedGap` `[yarns]`, default 0.** NBS C431 says some bichrome ribbons are
  inked so a white "pin stripe" is left between the colours, and it stays
  white. The gap is a strip of no ink, `/BleedGap` x `tYarn` (0.48 pt) tall,
  that is `/BleedGap` x 1000 x 0.48 / `/Size` glyph units: 40 at 12 pt, 38.4 at
  the default 12.5, 12 at `/Size 40`. It is centred on the colour boundary
  (inside the 60-unit blend, which is left alone) and cut from the other
  colour's pass, the main ink's passes and the clogged-counter fill, by one
  `rectclip` of the two rectangles outside the strip (`tYOut`), set in
  `tZonesBuild` (`tGapBuild`) only where the boundary reaches the face.
  `tZGap` is reset to null at the start of every strike's build, and guarded
  by `tZoned` in the clog, so a strike the boundary misses never inherits the
  last strike's strip.
- It needs `/Bleed` above 0 (at `/Bleed 0` the boundary is 360 units below
  the baseline, outside every glyph) and does nothing for a strike the
  boundary does not reach.

## Documented, not changed

- **`/WornBand`'s sign.** The three board scans measured by `tools/lane.py`
  show the tops the same or slightly lighter (peak -2/-4/-5%, mean
  -1.5/+1/-8%), never darker, so a grey middle with dark tops is an artistic
  option. `/WornBand 1` takes about 15% of the lane's ink (0.3 about 4.5%),
  more than any difference seen and with the opposite sign. Making it
  "neutral" would change every `/WornBand` render and has no measurement
  behind either sign, so the code stays and the `@param`, the comment and
  TYPEWRITER.md say so.
- **`/Bleed`.** Default stays 0 (C431: most bichrome ribbons show no
  diffusion). The depth is `tBleed` x 540 glyph units (the issue's 350 was
  out of date). Black into red is the documented direction; a red foot on
  black text is the mechanism only.

## Checked

- Defaults byte-identical: `scripts/identity_check.sh --loader
  lib/typewriter.ps` (3 files incl. the typewriter specimen) identical, with a
  control that uses `/BleedGap` (gated on `TWState /tBleedGap known`) and
  differs between the trees.
- `tests/typewriter.rs` `bleed_gap_leaves_a_white_stripe_at_the_boundary`:
  gap 0 is the page without; a gap with `/Bleed 0` moves nothing; a 3-yarn
  gap blanks 5 rows at 300 dpi at `/Size 40`, 1 yarn blanks 1 (2 px minus
  antialiasing); `/above` differs too. Guards in the `tw-setup-guards`
  selftest.
- Demo: `docs/dev/design/typewriter/ribbon_extras_538.ps` / `.png` (7x, so a
  yarn is visible). At 12 pt and 144 dpi a single yarn is about a pixel.

## Not done

- No tuning of `tBlTr`, `tBleed`'s 540, `tWornD` against a scan: none exists.
- No bichrome preset.
