## Bare pigment names in the colour keys (issue #356, 2026-10-14)

`/Colors`, `/HighlightColor` and `/ShadowColor` now take a literal pigment
name (`/Colors [ /sapgreen /cadmiumyellow ]`, `/HighlightColor /titaniumwhite`)
as well as `[r g b]` arrays. This is option (a) of the issue, in the L1
resolver only (`puparse`, `puhue`); no preset body changed.

How: a new `@internal` helper `pupig` reads `Pigments` directly and returns a
fresh `[r g b]` (not an alias of the palette's entry). `puparse` resolves names
after the 1-to-8 length check and before the rgb checks, so the record and
every later check see only arrays. It copies the list only when a name is
present, so a load without names is still the caller's own array and the
record is unchanged (identity). `puhue` resolves a literal name before its own
checks.

Guards (a guard is an undefined name, so the text is the name):

- `<preset>-colors-unknown-pigment-see-PigmentNames` and
  `<prefix>-unknown-pigment-see-PigmentNames` for a name not in `Pigments`.
- `<preset>-colors-pigmix-spec-needs-pigcolors` and
  `<prefix>-pigmix-spec-needs-pigcolors` for a literal array led by a name (a
  `pigmix` spec). Checked before the length test, so `[ /sapgreen 1 /x ]` now
  gets this instead of `...-components-must-be-0-to-1`.
- `-colors-entries-must-be-rgb-arrays` is unchanged for everything else
  (numbers, procedures, empty or short arrays).

Decisions:

- **No clash.** artkit's `Palettes` (`dusk`, `ember`, ...) are read by `pal` /
  `palpick` only, and no lib other than paintkit has a table with a pigment
  name. A name that was refused before (`/Colors [ /x ]`) is now accepted only
  if it is a pigment, so no previously valid input changes meaning.
- **Specs stay behind `pigcolors`.** Resolving specs inside the keys would
  hide a mix recipe in an option and was not asked for.
- **Not done:** `/Under` (pkwet) and `/Tone` (paper) are rgb options outside
  the L1 resolver and still take arrays.

Verification: `colour-keys-take-bare-pigment-names` `%%SelfTest` (resolution,
no mutation of the caller's array, no aliasing of the palette, the guards, the
unchanged old guards, `[ [ ] ]` not crashing); three tests in
`tests/pigment.rs` (pixels equal the `pigcolors` form through `pkfan`, `pkoil`
and `pkdab`; guard names through five presets); `scripts/identity_check.sh
--loader lib/paintkit.ps`.
