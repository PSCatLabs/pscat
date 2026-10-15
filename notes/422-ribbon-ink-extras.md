# #422: ribbon ink extras, the worn band and two-colour bleed

Follow-up to #212 (`notes/212-ribbon-ink.md`) and its tuning in #423, #477 and #511.
Design in `docs/dev/design/TYPEWRITER.md` section 6, "Added in #422". Library:
`lib/typewriter.ps` (`tZonesBuild`, `tZoneDo`, `tRun`, `tInkStrike`). Feeds the
capstone #531 (a final pass on `gallery/released_memo.ps`).

## What shipped

Four new keys on `tw-setup` / `tw-set` / `tw-type`, all off by default:

| key | meaning |
|---|---|
| `/WornBand [0..1]` | the ribbon lane every x-height strike hits (glyph heights 20 to 410) prints lighter |
| `/Bleed [0..1]` | how far a black/red ribbon's colour boundary reaches into the face |
| `/BleedInk [rgb]` | the other half's colour (default red) |
| `/BleedSide /below, /above` | which side the other half lies (default `/below`) |

Neither extra runs any new code when its key is 0, and the default call path
(`tZoned` false) is the old `gsave level weave body grestore`, so every existing
page is byte-identical (identity check below). They apply to ribbon ink only.

## Design (and what the advisor changed)

- **One mechanism, a band at a fixed height in the cell.** `tYoff` (line + machine +
  strike vertical offset) is taken out of the band's edges, so a bent slug or a high
  strike moves against the band: it reads as a property of the ribbon, not the glyph.
  No extra hash slot is needed for the worn band.
- **Bands overlap by 20 units, never abut.** Abutting clipped draws leave a light AA
  hairline at the seam in pscat. A first version drew nested passes over the whole
  glyph (a sparse pass over everything, a denser one over less); its AA edges were
  drawn up to three times and darkened the glyph outline 6% outside the band
  (`the ascender kept 1.059`). Bands that overlap only at their joins keep the ascender
  at exactly 1.000 of the page without the key.
- **Bleed is a colour change past a boundary, not a halo.** The first model was a
  ghost of the glyph in the other colour, offset toward the boundary and drawn under the
  main ink. It was wrong: on a real two-colour ribbon the part of the type face that
  lands past the boundary prints in the other colour. The other colour is now laid
  first through the weave (hole level 0.17 below the main ink's, so its voids are a
  subset of the main ink's), and the main ink goes over it through a sparser weave in
  a transition 60 units either side of the boundary in three steps, so the other colour
  shows through the voids and the edge is ragged at yarn scale. (A first transition of
  +-30 units in one step read as a ruled line at /Size 40.)
- **The boundary wanders** per strike from a hash (slot 22 of the strike prefix, mixed
  2:1 with a per-key draw 22 of the machine prefix; the key alone when `/StrikeWear` is
  0). It rests 360 units below the baseline (`/above`: 800), far enough that the
  boundary plus its transition (80 units) and the strike's own vertical offset (tens of
  units) is past every descender (ascender) tip, and moves into the face by
  `/Bleed` x 540 x u. (A first version rested at -200 / 690:
  the transition reached the descender tips, so every `/Bleed > 0` tinged every
  descender; the advisor review caught it, and the byte-identity test now has
  descenders and ascenders on both sides.) A strike the
  boundary does not reach takes the unzoned path: it is the page without `/Bleed` to
  the byte (a test: `/Bleed 0.0001` on `Hxdlbk` equals `/Bleed 0`).
- The hash generator `hA hB` is saved and put back around the new draws, so turning a
  key on changes only that effect.
- A procedure fetched by name runs: `zdPr` and `trPr` are used as `/name load exec`
  (hit during the work: the proc ran twice).
- Not done: a clogged counter's fill stays in the main ink in the bleed zone.

## Reference check

There is no two-colour sample on the reference board (REFERENCES.md; TYPEWRITER.md Q3).
A bounded web search (retail listings, Wikipedia "Ink ribbon", a ribbon-care page) found
only that a selector "slightly raises or lowers the ribbon" so the type face hits the
chosen half, and nothing on colours mixing at the boundary; a ribbon-care page confirms the
selector picks the top or the bottom quarter inch of the half-inch ribbon and does not
say which colour is which (a search summary's claim that black is usually the lower half
was not on the page), so `/BleedSide` is a key and the default `/below` is a guess. So the bleed is a model of the mechanism, tuned by eye, not
measured against a scan. The worn band has the same standing: T10 (a worn grey-brown
ribbon) is lighter overall; a lighter lane is not measured on a scan either. Gaps filed
as p3 (below).

## Verification

- `scripts/identity_check.sh` against the merge-base tree (details in the PR): the
  released specimen and memo are byte-identical; the control (a driver that opts in)
  differs. The typewriter specimen changes (a new block), so it is checked another way:
  the base specimen renders identically through the head's `lib/typewriter.ps`.
- `tests/typewriter.rs`: `the_ribbon_extras_are_off_by_default_and_deterministic`
  (keys at 0 = the page without them; on = a pure function of the seeds; the caller's
  `rand` untouched), `the_worn_band_lightens_the_lane_and_not_the_ascender` (the lane
  keeps 0.85 of its ink, the ascender exactly 1.0; flat ink untouched),
  `bleed_prints_the_other_colour_below_the_boundary`.
- Ghostscript renders the specimen without error (correctness only, not timed).

## Cost

Only pages that opt in pay: a banded strike draws its body and feather about 4 times
(worn band) or 5 (bleed, 4 main bands and the far one), 12 or more with both. No
timing was done (shared machine); the default path is unchanged.
