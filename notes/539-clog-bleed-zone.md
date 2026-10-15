# #539: a clogged counter takes the bleed colour in the bleed zone

With `/Bleed` on, a clogged key's counter fill (`tClogBand`) was drawn in the
main ink even where the strike's boundary puts that part of the letter in the
ribbon's other colour, and the worn lane did not lighten it. The feather and
the body already ran under `tRun`, which is `tZoneDo` when `tZoned` (the
strike's bleed and worn zones) and one `tLevel tWeaveClip` otherwise.

## Change

- `tClogBand` stores its fraction in `tCFr` and hands the fill (translate,
  rotate, `rectclip` of the band, glyph counters, `fill`) to `tRun` as a
  procedure. `tZoneDo` runs it once per zone, so the fraction cannot stay on
  the stack. The `fraction > 0` guard stays outside `tRun`: the 0.12-short band
  can be negative.
- The explicit `tZoned { tZGap ... rectclip }` in `tClogBand` is gone: `tZoneDo`
  applies the pin stripe (`tZGap`, #538) to every pass already.
- Unzoned (the defaults, or a strike the boundary misses and no worn lane) the
  path is `gsave p tLevel tWeaveClip proc grestore`, the operators the old
  `tClogBand` ran, in the same order, so those pages are byte-identical.

## Side effect, intended by the issue

`/WornBand` alone now lightens a clogged counter that sits in the lane (the
issue lists it: "the worn band does not lighten it"). The fill is the only ink
in the counter, so at `/WornBand 1` a clogged `o`'s counter holds about 9%
less ink. Pages that set `/WornBand` above 0 and have a clogged key change;
nothing else does.

## Checked

- `tests/typewriter.rs` `a_clogged_counter_takes_the_bleed_colour_and_the_worn_lane`:
  machine 4's clogged `o`, with `/Bleed` poked to 1.6 (at the option's 1 the
  boundary never gets below 528 units, above the counter's top at 390), over
  the seeds whose boundary lies at 150..300: no dark neutral pixel in the rows
  past the blend, and red there; and the counter patch is lighter in the lane.
  Fails on the base lib, passes on this one.
- Identity: `scripts/identity_check.sh --loader lib/typewriter.ps
  --loader lib/released.ps`, with a control that sets `/Bleed` (and differs).
  `released_specimen`, `released_memo` identical. `examples/typewriter_specimen.ps`
  DIFFERS, only in the rows of its "ribbon extras" block (it sets `/WornBand`
  and `/Bleed`; the rest of the page is identical), so
  `docs/dev/design/typewriter_specimen.png` is regenerated (its committed copy
  matched the base render exactly; `ribbon_extras_538.png` is unchanged, it
  has no clogged key in the zones).
- Demo: `docs/dev/design/typewriter/clog_bleed_539.ps` / `.png`, before and
  after. The boundary is poked past `/Bleed 1` (`TWState /tBleed 1.6 put`) so it
  crosses the counter: at `/Bleed 1` it never gets below 528 units on `/above`.
