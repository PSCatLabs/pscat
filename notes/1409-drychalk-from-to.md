## drychalk /From /To: draw part of a stroke (issue #1409, 2026-11-24)

`<< /From 0.0 /To 0.4 >> dcchalk` draws the first 40% (by arc length) of each
stroke with the pressure profile, width variation, texture and dust the whole
stroke has there. Motivation: writing over time (psanim's stroke scheduler,
PSCatLabs/psanim#30) used to draw the whole stroke each frame, clipped to a band.

**Design.**
- The whole stroke is worked out first (`dcL`, dot or not, the stroke's seed and
  its `dcr1..3` draws, `dcfirmprofile`), after `/Hand` and `/Corners`, so "from 0"
  includes the hand's run-in. Only the polyline given to `dkstroke` is cut
  (`dcpiece`, over `dctrim`, which is unsafe on an empty range: it returns the whole
  polyline, so empty is decided first). `/Touch` and `/Pressure` are wrapped to read
  `From + t (To - From)`, so the piece has the profile of its place in the stroke.
- Texture is position-hashed in drykit (canvas cells), so it matches wherever the
  piece is drawn. drykit's walker places stops from the piece's own start, so a
  seam is within a few tens of levels (measured: max channel difference 26 of 255 on a tip
  stroke over a two-piece split, 36 over three), and the pen's last pitch shows the
  band of touch at the cut, not the next stop's.
- Dust: every speck's random draws are made in order for the whole stroke; a speck is
  filled only if its arc position is in `[From, To)` (`To` 1 closed). Pieces
  partition the specks exactly.
- A dot (a stroke under 1.5 widths, or zero length) is atomic: drawn whole by the
  piece with `From <= 0.5 < To`.
- With `/Tilt`, each lane is cut by its own length; a piece with `From > 0` gets the
  whole lane's seed (so a seedless call agrees with the whole stroke) but its on/off
  dropout chain starts afresh there, seeded by the lane's seed mixed with `From`
  (the lane's seed alone left a lane that starts off off in every incremental piece: ink 0.60
  of the whole at 100 pieces for `/Seed 1`; now 0.98 to 1.02 over seeds).
- Defaults leave the code path untouched (`dcPart` is false): identity-checked.

**Growth, measured.** `/From 0 /To t` frames 3-4% apart on a 180pt line (0.3 to 1.0, no
`/Seed`): pixel-identical behind the pen's last 12 units in every transition but one, where
the stroke crosses a drykit chunk boundary and its edge rows change once (318 pixels, up to
42 of 255; texture untouched). Frames far apart (0.5 vs 1.0 on a curve): edge pixels up to 26.
Adjacent pieces on one canvas (`/From i/N /To (i+1)/N`): N=20 and N=100 keep the weight
(mean ink luma -3 and -6), N=300 (pieces 0.26 widths) -14, N=1500 (0.05 widths) visibly
greyer, since each piece ends in a round cap that overdraws the last. `dcpiece` therefore
drops only ranges under a millionth of the unit (where `dctrim` would hand back the
whole), never slivers, so a slow pen leaves no gap. Proposed drykit follow-ups: stable
chunking, cap-less interior ends.

**Cost.** A tenth of a long multi-stroke path (8 repeats of a 1600-unit zigzag path, 144 dpi)
renders in 0.78 s against 6.1 s for the whole (release, this machine; includes
the board and start-up). The dust still draws every random number of the stroke it
skips, which is cheap beside the catch.

**Not done.** `dkstroke`, `dycrayon`, `dhstroke`: their profiles are not shaped like
dcchalk's (a touch/pressure remap) and live in drykit, drycrayon and drycharcoal, which
this ticket does not own.

**Update (#1419).** The one-frame edge shift was not the chunk boundary: drykit inks a constant-width run with a round-capped stroke and a varying one as a filled union, and a cut run switched branch when it first reached a change of width. Fixed in drykit, and tip strokes now let drykit cut the whole stroke (`notes/1419-drykit-stable-growing-parts.md`).
