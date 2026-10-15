# #439: mathkit columns and braces

Filed from #340 (PR #440). All of #439 shipped in one PR. No committed
render changed (see Evidence). One new specimen.

## What was added (lib/mathkit.ps)

- **`\mod{n}` and `\pod{n}`.** amsmath's other congruence forms, beside
  `\pmod`. `\mod{n}` is an 18 mu space, an upright "mod", 6 mu (`\,\,`)
  and n, with no parentheses. `\pod{n}` is 18 mu and (n), with no "mod".
  amsmath uses 18 mu only in display math and 12 mu (8 mu for `\pod`)
  inline. mathkit's `\pmod` already used 18 mu everywhere, so these do
  too, and `\pmod` is unchanged.
- **`\nolimits` (and `\limits`) on a brace.** `mkclimits` now accepts the
  `/brace` atom as well as `/op` and `/fn`. `\underbrace{x}\nolimits_{k}`
  sets the label beside the brace as a script, through `mkscripts`, as
  for an ordinary atom. `\limits` stays the default.
- **`/ColumnWander` (mkalign).** With it, each right-left pair after the
  first drifts sideways on its own, line by line. Default 0.
  - Each line's offset for pair k is 0.6 times the previous line's, plus
    a step of up to ±0.07 em × /Irregular × /ColumnWander. The step is
    drawn from that line's own seed (stream `-3 k`). So a column creeps
    rather than jumps, and editing a later line never re-rolls an earlier
    one.
  - Pair k carries the drift of the pairs before it, so the pairs never
    cross. Each pair's own step is clamped so that it closes on the pair
    before by at most half the gap plus 0.05 em. At `/ColumnGap 0` that
    leaves 0.05 em of leftward drift, about as much as the irregular
    gaps between atoms.
  - It is folded into `mkcolumns`' per-pair shift, so `/Width`, `/BBox`,
    `mkframe` and `mkunderline` follow it.
  - The first column pair never moves (the line's own `/Jx` already
    wanders it).
- **`alignat`.** `/ColumnGap 0` is exactly amsmath's `alignat`: the pairs
  butt, with no space between them. That is now documented and tested
  rather than added as a new option.
- **`mkbrace`.** `x0 y0 x1 y1 opts mkbrace -> xt yt`. It draws a curly
  brace whose arms end at the two points, and returns the tip of its
  point, where a label goes.
  - The shape is strokeglyphs' grown left brace (`sggrow /braceleft`),
    the one `\overbrace` draws, under a CTM turned to the chord's angle
    plus 90°. The CTM is saved and restored around it, never with
    gsave, which would drop the path.
  - The point faces right of the direction of travel. Drawn left to
    right it points down (under a span), and right to left it points up.
    This is the same handedness as `mkarrow`'s `/Bend`.
  - `mkgrowink`'s dry run gives the ink's extent across the brace. The
    arm tips (the ink's right edge) go on the chord, and the returned
    point is the ink's left edge.
  - `/Irregular` turns it up to 1.2° off the chord, starts it up to 2%
    early or late, and makes it up to 3% short or long. At
    `/Irregular 0 /Jitter 0` the arms land exactly on the points. The
    default `/Jitter` still wobbles a 200 pt brace by a point or two,
    which is the hand.
  - It uses the shared annotation options (`/Size /Seed /Irregular
    /Jitter`) and seeds like `mkarrow`. It never touches the formula
    drawing's drift state (`mkdrift`, `mkG`): those are left over from
    the last formula.

## Design calls

- **`/ColumnWander` is off by default.** On by default, it would move the
  align block of the committed `mathkit_structures` render. Each line
  already wanders as a whole (`/Jx`). The per-column drift is for a board
  that wants more of a hand.
- **The drift is a decaying walk, not independent jitter per line.**
  Independent offsets read as noise. A hand writing a second column down
  a board creeps one way, then back. It does that because each line
  starts from where the eye left the last one.
- **No `/Side` option on `mkbrace`.** To flip the brace, swap its two
  points, as you would reverse an arrow.
- **No thicker brace for long spans.** The builder's thickness tops out
  at 0.28 em, which is the same shape as `\overbrace`. A bigger `/Size`
  draws a bolder one.

## Behaviour change to know about

`mod` and `pod` are now command names. A bare run of those letters in
math raises `mk-bare-word-needs-a-backslash:mod` (or `pod`), where it
used to draw the letters. `\text{mod}` is unaffected. No tracked file
writes either word bare (checked with `git grep` over examples, gallery,
selftest drivers, tests and docs).

## Tests

- Three `%%SelfTest` blocks:
  - `mod-pod-and-braces-beside`: the widths of `\mod` and `\pod` against
    hand-built equivalents; a side label stands lower (shallower) than a
    limit; `\limits` is the default; everything lays out under
    `/Slant 10`; three refusals.
  - `mkalign-alignat-and-column-wander`: `/ColumnGap 0` butts the pairs;
    `/ColumnWander 0` is the layout without it; there is no wander at
    `/Irregular 0`; with it the second pair moves by less than a quarter
    em while the first column stays; a negative value is refused.
  - `mkbrace-marks-a-span`: the arms land on the points; the point is
    half way along, as deep as the brace's ink, and faces right of
    travel in four directions; it leaves only the point on the stack;
    three refusals.
- `tests/mathkit.rs`:
  - The new specimen renders with ink and an empty stack.
  - Ghostscript renders the specimen, and builds the same layout and
    path (within 0.02 pt) for `\mod`, `\pod` and side labels, and for
    four `mkbrace`s (diagonal, down, right to left, up), with their
    returned points.
  - The gs number comparison is now its own helper,
    `assert_gs_same_numbers`.
  - The example-tag count is now 7.

## Demo

`examples/mathkit_columns_braces.ps` is the new specimen, rendered at 2x
as `docs/dev/design/drymedia/mathkit_columns_braces.png`, with a
`specimens` entry in the gallery manifest. It shows:

- the congruences;
- the side labels next to the stacked ones;
- the same five three-pair lines without and with `/ColumnWander 1.5`;
- an `alignat` system with its coefficients in columns;
- a drychalk board with braces under and over spans of an expansion,
  down the side of a system (pointing at its solution), and under a
  sum at `/Irregular 2`.

## Evidence

- `scripts/identity_check.sh <base worktree> --loader lib/mathkit.ps
  --control <driver>`. See the PR for the exact counts. The control
  draws `\mod`, `\pod`, a side label, a wandering column and an
  `mkbrace`, each under `stopped`, so the base tree doesn't error.
  Every committed mathkit render is byte-identical, and the control
  differs.
