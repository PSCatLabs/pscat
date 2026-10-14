## paintkit pigment palette and paint-like mixing (issue #185, 2026-10-14)

`artkit`'s `mix3` is an RGB lerp: blue and yellow lerp to an olive grey, white
only lightens, a dark mix never goes rich. paintkit now has `Pigments`, a
named oil palette of thirteen colours, and `pigmix`, which mixes them like
paint. (Epic #112; the landscape agents of #118/#187 hand-picked RGB.)

What was built (all in `lib/paintkit.ps`, a section before the `pu-`
resolver; no preset changes):

- `Pigments` (name -> `[r g b strength]`, a `@kind: Dial` like `dybox`) and
  `PigmentNames` (tube order).
- `pigment` (`/name` -> r g b), `pigmix` (`[ /name w ... ]` -> r g b),
  `pigtint` / `pigshade` (`/name t` toward white / black), `pigcolor`
  (-> `[r g b]`) and `pigcolors` (a whole `/Colors` list).

The mixing model: Kubelka-Munk's two-constant theory per channel. A pigment of
reflectance R has K/S = (1-R)^2 / 2R. A mix with weights w_i and strengths s_i
(a_i = w_i s_i) has K/S = sum(a_i K/S_i) / sum(a_i), and reflectance
R = 1 + K/S - sqrt((K/S)^2 + 2 K/S). Strength is the single per-pigment weight
that matters (white 12, a plain colour 1, phthalo blue 1.6). That is a few
multiplies and three square roots; A three-pigment mix measures about 8 microseconds in pscat and 2.4 in Ghostscript. The
pigment colours are the as-read-on-white values, tuned by eye against the
reference mixes in `tests/pigment.rs`: blue + yellow green, blue + alizarin
violet, + white lavender.

Decisions:

- **Not a new library, not `Palettes`.** The issue says "paintkit", and
  `build.rs` rejects `@kind: Palette`. Following `drycrayon`'s `dybox`/`dycolors`.
- **No change to `puparse`/`puhue`.** `pigcolor`/`pigcolors` produce the arrays
  the colour keys already validate, so no guard moved and identity is trivial.
  The price is one wrap (`[ ... ] pigcolors`) instead of bare names in
  `/Colors`. A bare name in `/Colors` is still `pkfan-colors-entries-must-be-rgb-arrays`.
- **Gamma-encoded mixing**, not linear light: no `pow`; limits documented.
- A first tune had violet landing on navy (G above R) and a warm white; both
  were retuned (alizarin bluer, phthalo lighter, white neutral-cool).
- A claim that "tints lighten faster than the lerp" turned out false for strong
  dark pigments (phthalo blue at t=0.4 is darker than the lerp); the test caught
  it and the docs now say only what is true: a tint is not a straight line to
  white, and it loses saturation.

Limits (also in the library header): three bands not a spectrum; complements
(phthalo blue + bright red) fall to a very dark neutral; encoded not linear;
a pot of paint, not a glaze; weights are parts.

Verification: two `%%SelfTest` blocks (the mixes, scale and order freedom, 20
guards), `tests/pigment.rs` (pinned mixes, green/violet/tint claims, every pair
of pigments valid, no random draws, CPU budget, `pigcolors` equals the hand
written rgb through `pkfan` pixel for pixel, catalog entries, Ghostscript
agreeing to 1e-5 and accepting the specimen), and `scripts/identity_check.sh`
against the base for every file that loads paintkit.

Not done: no site card (the specimen is in the README and `docs/`); no
spectral or glaze model; `/Colors` does not take bare pigment names.
