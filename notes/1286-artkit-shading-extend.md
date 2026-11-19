# 1286: artkit axialsh/radialsh pad by default

Since #1040 a shading dict with no `/Extend` paints nothing past its ends (PLRM, gs). artkit's
`axialsh` and `radialsh` build their dicts without the key, so a caller whose clip reaches past the
axis got an unpainted band. (The issue calls `axialsh` "linearsh"; there is no `linearsh`.)

**Decision (reverses part of #1040's note).** #1040 left artkit's helpers at the spec default and
documented `dup /Extend [true true] put` for padding. That put the burden on every caller, and the
two #1040 corpus files and the two hybrid examples (#1251) had all needed the key. The helpers are an
art convenience layer, and a gradient fill that stops at its axis ends and leaves paper is almost never
what art wants (CSS and SVG `pad` do the opposite). So both helpers now put `/Extend [true true]` in
the dict they return. A raw `shfill` dict is untouched: it keeps the PLRM default. A caller that wants
the bounded gradient says `dup /Extend [false false] put`.

**Callers audited** (`git grep 'axialsh\|radialsh\|gradfill'`):

- `gallery/varsity_backs.ps`: the first three jersey layers are stacked bands, each meant to cover only
  its own span. Padded, the two sleeve bands would flood the whole body with their end colour, so they
  (and the five-stop centre layer) now say `/Extend [false false]`. Render byte-identical to before.
  The padded layers (line 95) and the backdrop (`0 0 0 880` on a page 660 tall at the 1400x880 grid
  scale) were unaffected.
- `gallery/firefly_census.ps`: the sky axis stops at y 150 but the clip is the whole page; the strip
  below 150 is overpainted by the hill and grass, so nothing changes.
- `examples/gradients.ps`: panels 3 and 4 no longer need their own `/Extend`; panel 2's clip is exactly
  the axis. Byte-identical.
- `lib/lettering.ps` `/transition`: the axis is the glyph box's diagonal, so the whole box lies inside
  t in [0, 1]. Identical.
- `examples/functional/type-common.ps` (used by `type-terrain.ps`, `observable-press.ps`): identical.

`scripts/identity_check.sh --loader lib/artkit.ps` (plus the two functional examples) over every file
that reaches artkit: 110 of 110 pages identical (the control, which keys on `/Extend`, differs as it
must). So no committed render changes; the fix matters for future callers
and for any piece whose geometry grows past its axis.

Tests: `tests/artkit.rs` asserts the `/Extend [true true]` key on both helpers and that `gradfill`
pads past the ends, and that `[false false]` leaves the paper bare.
