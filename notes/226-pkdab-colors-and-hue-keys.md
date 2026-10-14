## Paint loading L5: `pkdab /Colors`, `/HighlightColor` and `/ShadowColor` (issue #226, 2026-10-13)

Slice L5 of #183's design (`docs/PAINT_LOADING.md`, sections 2-3 and 6). `pkdab`
takes `/Colors` and `/ColorMix`, one colour per dab, and `pkdab` and `pkoil` take
`/HighlightColor` and `/ShadowColor`. With the keys unset every render is
byte-identical (nine pinned scenarios, plus `scripts/identity_check.sh` across
every driver and the alpine_lake still); no branch runs unless its key is set and
nothing draws a random number.

- **A dab's colour is its slot-5 draw.** The outline phase `pdd 5` is already a
  0..1 draw per dab; `puat` is called at `u = 1 - 2p` (so `p` is exactly the
  draw), flip `false`, then `puwet`, once at generation. It lives in slot 14 of
  the dab array (the array is one slot longer), so the shadow, body and
  highlight layers all read the same value. The unset path stores the caller's
  colour there, which is the same array the layers read before.
- **`puparse` is called always, `pulightneedscolors` never.** Gating `puparse` on
  `/Colors` being set was considered, and would leave the unset path without the
  spelling and `/ColorMix`-needs-`/Colors` guards unless they were repeated.
  `puparse`'s unset branch is exactly those two checks, and it never reads
  `/Light`, so calling it unconditionally keeps the guards without duplicating
  them. A self-test pins
  that `/Light 45` alone still passes: `/Light` is pkdab's own.
- **Shadow and jitter.** pkdab folds `/ColorJitter` into the shadow layer's
  `shade` factor. With `/ShadowColor`, the layer mixes toward the target by
  `0.55 * /Shadow` and the jitter is applied after as its own `1 + j` shade, which
  keeps `/ColorJitter` meaning what section 2.4 says. The highlight layer has no
  jitter, so it is `mix3(c, H, 0.6 * /Highlight)`. `pkoil`'s layers are
  `mix3(base, H, /Highlight)` and `mix3(base, S, /Shadow)` as in section 3.2.
  Unset, both still `shade` toward white and black: `mix3` toward `[1 1 1]` is not
  byte-equal.
- **The mix targets are graded under `pkwet` too.** They are absolute colours; mixing a
  graded body toward an ungraded target would leave the lit and shaded fractions
  solid in the outer passes of a pull. `puwet` is affine, so grading `H` and `S` once
  at the top of the call and mixing equals grading the mixed result, and outside a
  `pkwet` it is the identity. The design text does not say this; HANDOFF's rule
  (every loaded colour goes through `puwet`) requires it. Both presets, pinned
  by a pull test with a mutation check on each of the four `puwet` calls.
- **`puhue`** is the one new helper (`pu`-prefixed, `@internal`): `opts /Key
  (preset-key) puhue -> [r g b] | null`, with guards `pkdab-highlightcolor-must-be-an-rgb-array`
  and `...-components-must-be-0-to-1` (and the shadow pair, and the `pkoil` four).
  The design lists no guard names for these keys; these follow its scheme.
  `pkoil` also calls `puspell`, so `/HighlightColour` is rejected rather than ignored.
- **`/Light` on pkdab still moves the layers.** The record's `/Light` is not used
  (no flip: a dab has no head); `pdlight` keeps placing the shadow and highlight
  layers, so `/Colors` plus `/Light` is the existing placement with new colours.
- Tests: design test 7 (`highlight_color_moves_the_hue`, both presets, each key
  moving only its own layer, and the two ridges on opposite sides) and 8
  (`pkdab_colors_mix_per_dab`: a 400-dab `[red blue]` clump splits 35-65%, and
  `/ColorMix 1` grades), the draw count and dab mask with the keys on, a uniform
  load equal to the plain brush (byte for byte, with jitter and both side layers
  on), the guards as self-tests, and the grading under `pkwet`. Mutation-checked:
  dropping each `puwet`, using a constant head position, and ignoring either
  target each fail a test.
- Not here: the specimen and the gs driver over every adopter (L5b, #227),
  `pkdry` and `pkoil` ridges (L7).
