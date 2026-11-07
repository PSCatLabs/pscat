## Chalk by hand: /Hand, /Corners, dctext and dcleader (issue #1049, with #1054, 2026-11-07)

**#1049: `dcchalk` /Hand.**
- Lettering from `sgpath` already had a hand's irregularity, but every line,
  box and circle a piece built itself came out ruled.
- `/Hand` (0..1) runs a pre-pass over the flattened subpaths before any stroke
  is drawn. Because of this the dust follows the drawn line and `/Seed`'s
  "subpath i" still means the i-th stroke drawn.
- **Drift.** Each stroke drifts to the side with dywobble's shape, two sine
  waves under a slower envelope, minus the crayon's hooked end.
  - The amplitude is `0.5 w + 0.005 L`: in stroke widths, plus a little more
    for a longer stroke, because a hand drifts further over a long edge than
    over a short one.
  - The wavelength is `24 w + 0.6 L`, so a long edge gets one or two slow
    bows rather than a ripple.
  - The tremor is at most a twentieth of a width. The issue found
    dywobble's 0.25 pt tremor crinkly on long edges.
- **Ends.**
  - An open stroke starts up to half a width early and runs straight on up
    to a width and a half.
  - A closed outline, whose last point equals its first, ends up to two
    widths past its start or one width short.
- **Short strokes are left alone.** Everything is scaled by
  `clamp((L - 4w) / 40w)`. A dot or a stroke under four widths comes back
  as the identical array, and most glyph strokes get a fraction of a point.
  That is why text needs no "is this sgpath?" detection.
- **Sampling.** The vertices are kept, and only segments longer than two
  widths are subdivided. Resampling at a fixed pitch would chop flattened
  curves (advisor review), and it would change the polyline, and therefore
  the render, even at zero amplitude.
- **Default 0.** `/Hand 0` skips the pre-pass entirely, so the gallery
  lecture, mathkit, `pscat chalk` and every existing board are unchanged
  (identity check below). The issue asked for an option. Making it the
  default would have changed every chalk render in the repository and in
  users' files.
- **Randomness.** The wobble draws only from drychalk's own `dcrand`. It is
  seeded from the stroke's first point plus a salt of 250007, kept apart
  from the stroke's, the dust's and the lanes' seeds so the drift and the
  texture never correlate, or from `Seed + 7919 i + 29`. The caller's `rand`
  is untouched, and the SelfTest checks it.
- **`/Corners /separate`** (the issue's item 4) cuts a subpath at vertices
  that turn more than 50 degrees between two legs at least ten widths long.
  A box splits into four strokes. A flattened curve's short segments never
  split, and neither do most glyph corners (and `dctext` drops the key).

**#1054, accepted part: sign lettering.**
- `dctext`, called as `str x y opts`, is `sgpath` measured with `sgwidth`
  and drawn with `dcchalk`.
  - It takes `/Align /left|/center|/right` and `/Fit` w, which sets
    `/Size` so the measured width is exactly w.
  - `/Slant /Jitter /Strict` go to `sgpath` and `/Seed` goes to both. The
    rest goes to `dcchalk`, and `/Width` defaults to 0.048 × size, the same
    as `pscat chalk`.
- `dcleader`, called as `left right x1 x2 y opts`, is a menu, price-list or
  contents row.
  - The leader dots are zero-length subpaths, so `dcchalk` draws them as
    pressed dots. By default they have 0.8 of the lettering's width and
    strength, and `/Leader << >>` overrides that.
  - The dots sit on whole multiples of `/Pitch` from x = 0, the way TeX's
    `\leaders` aligns to the enclosing box. Rows with the same pitch line
    their dots up, which a test pins.
- **Declined per the triage:** `dcboard /Frame /wood`, `/Nails` and the
  doodle motifs. They are one sign's design, not general primitives.
  `examples/drychalk_hand.ps` draws its frame with plain `dcchalk /Hand`.

**Verification.**
- SelfTests cover the guards for every new key and procedure, and confirm
  that the hand, `dctext` and `dcleader` take no draws from the caller's
  stream.
- `tests/drychalk.rs` checks:
  - `/Hand 0` matches the ruled stroke byte for byte;
  - a long line drifts more than 3 px with `/Hand 1` and less than 1.5 px
    without it;
  - short strokes and dots are unchanged;
  - `dctext` aligns and fits;
  - the leader dots line up across rows.
- The specimen is `examples/drychalk_hand.ps`, rendered as
  `docs/dev/design/drymedia/chalk_hand_specimen.png`.

**Deferred.**
- A gallery or specimen entry in `gallery/manifest.json` (a different lane
  owned the generated files this wave).
- The skill docs (`plugin/skills/psart/references/dry-media.md`).
