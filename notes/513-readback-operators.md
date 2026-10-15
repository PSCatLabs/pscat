# #513: the missing readback operators (2026-10-14)

Found while working #505 (see `notes/505-grestore-bottom-currentgray.md`).
`currentcmykcolor`, `currentcolor`, `currentlinecap`, `currentlinejoin`,
`currentmiterlimit` and `currentstrokeadjust` were undefined in pscat.
Ghostscript 10.07.1 is the reference: each was probed in gs first, and
`tests/readback_operators.rs` pins its programs to gs's output (plus a
local-only re-run in gs, `ghostscript_prints_the_pinned_lines`).

## What each reads back

- `currentlinecap` / `currentlinejoin`: the integer `setlinecap` /
  `setlinejoin` took.
- `currentmiterlimit`: a real. It is held as `f32`, so it goes through the
  same shortest-decimal readback as `currentgray` (renamed `f32_readback`
  from `color_readback`): `3.3 setmiterlimit currentmiterlimit` is `3.3`,
  not `3.299999952316284`.
- `currentstrokeadjust` needed a setter, and pscat had none, so
  `setstrokeadjust` is new too. It stores a boolean (default `true`, as in
  gs) and **does not change rendering**: pscat's rasteriser does no stroke
  adjustment. It is part of the graphics state (`gsave`, `save`, a bottom
  `grestore`, `gstate` objects all carry it), and **`initgraphics` leaves it
  alone**, as in gs (`false setstrokeadjust initgraphics currentstrokeadjust`
  is `false`). A non-boolean is a `typecheck`.
- `currentcmykcolor` (four reals, through `f32_readback`):
  - a colour set in CMYK reads back as set (`0.1 0.2 0.3 0.4` is
    `0.1 0.2 0.3 0.4`, which the multiplicative RGB pscat paints with
    cannot give back), clamped to 0..1;
  - DeviceGray gives `0 0 0 1-g`, and so does a Separation or Indexed space
    over DeviceGray;
  - DeviceRGB gives `1-r 1-g 1-b 0`, with no black generation (gs prints
    `1 0 0 setrgbcolor` as `0 1 1 0`);
  - a Separation whose tint transform ends in CMYK, and an Indexed space
    over CMYK, read back the alternate's / the table's own components.
- `currentcolor`: the components in the current space. One for DeviceGray
  and for Separation (the tint), three for RGB, four for CMYK, and for
  Indexed the index as an **integer** (gs prints `[1]`, not `[1.0]`).
  `setcolorspace` gives the PLRM's initial colour: `0 0 0 1` in DeviceCMYK,
  index 0, tint 1.0.

## State added

`GraphicsState` gains `cmyk: Option<[f32; 4]>` (the components the colour
was set with; `set_rgb` clears it, `set_cmyk` sets it, so it is only ever
`Some` while it describes `rgb`), `spot: f32` (the Indexed index or
Separation tint) and `stroke_adjust`. Nothing reads them but the new
operators; `rgb` is still what paints.

## Deliberately not changed

- **`currentcolor` rounds in DeviceRGB, `currentrgbcolor` did not** (it does since #515, `notes/515-currentrgbcolor-readback.md`)
  (#505, #515): the rounding moved `alpine_lake.png` by one LSB at one
  pixel through `lib/paintkit.ps`'s arithmetic on `[ currentrgbcolor ]`.
  Nothing calls `currentcolor`, so the new operator prints gs's value.
- **`setcolorspace` still leaves the painting colour alone.** gs resets it
  (`0.3 0.4 0.5 setrgbcolor /DeviceGray setcolorspace currentcolor` is
  `[0.0]`); pscat keeps the RGB, so the same program reads back `[0.3]` in
  DeviceGray and `0.3 0.4 0.5` in DeviceRGB. The readbacks report what
  pscat paints; this asymmetry is not pinned either way. Only
  `/DeviceCMYK setcolorspace` records the initial `0 0 0 1` for the
  readback, without touching what paints.
- **CMYK to RGB.** gs converts additively (`0.1 0.2 0.3 0.4 setcmykcolor
  currentrgbcolor` is `0.5 0.4 0.3`), pscat multiplicatively (`0.54 ...`).
  Not touched: it would move every CMYK render.
- **A CMYK copy.** `currentcolor setcolor` in DeviceCMYK recomputes the
  painting RGB from the `f32`-rounded components, so in principle it can
  move the RGB by one `f32` ulp. A 1000-quadruple sweep found none, and
  the round-trip test compares the RGB as well as the components.
- **9-digit values.** gs prints `0.399999976` where pscat prints
  `0.39999998` for the same `f32` (see the #505 notes); the test compares
  the numbers inside a line as `f32`s.
- `1e10 setmiterlimit currentmiterlimit` prints `1e+10` in gs; not pinned.

## Evidence

- `tests/readback_operators.rs`, as above.
- Identity: no committed render changes (`scripts/identity_check.sh
  --base-bin/--head-bin`, results in the PR). No library or gallery file
  tests for any of the seven names with `known` or `where`.
