## hybrid showcase polish: the perceptual reviews' notes, and the sky drawn with shfill (issues #1029 and #1042, 2026-11-11)

One bundle, one re-render: both issues touch the same hybrid examples and the
same guide sheets.

**#1042: `sky_talk.ps` draws its pscat-alone sky with `shfill`.** `vramp` was
1 pt strips because PDF export used to flatten a shading to one colour (fixed in
#1028). It is now one axial shading per ramp with a FunctionType 3 stitching
function, a leg per pair of `[t r g b]` stops, clipped to the same band (the sea
ramp's clip runs 3 pt over the horizon, as the old strips did). Measured on the
`/PscatOnly` poster: PNG, the PDF rasterised by Ghostscript and the SVG each
differ from before by at most 1 to 3 levels per channel, so it looks the same
and is smooth where the strips were stepped; the PDF went from 321,244 to
308,174 bytes and now holds two `/ShadingType 2` with a stitching function each
(`tests/hybrid.rs` pins both). The hybrid's PDF (2,747,745 bytes) is the same to
8 bytes (the caption moved): the painted sky was never strips.
What stays strips, with the reason in the program: the soft shades under the
date and under the chart (a ramp of opacity: a shading ramps colour, and over
the painting there is no colour to ramp to), the sun's glow and the grain.

**#1029, the polish items.**
- *Hut dates.* `/Season /Autumn` said "Hut open 20 June – 20 September" over a
  mid-October photograph. The row now reads "Hut: closed until 20 June" in
  autumn. `tests/hybrid_reverse.rs` checks the autumn copy.
- *Route above the hut.* Both photographs show the zigzag trail, within a few
  pixels of each other, but the layout's `TrailHigh` turns were 1 to 6 px from
  it, and its first leg began 23 px from the trail, over the hut. The exact
  layer now routes along `RouteHigh`, corners read off the photographs on a 4x
  grid (to about 1 px). `TrailHigh` stays for the reference and for the
  pscat-alone picture, which paints it. The facts under the sheets now say the
  route is on the photograph's trail, not "on the points pscat laid out".
- *Leader to the col.* Every leader has a darker, wider halo (0.6 alpha, 3.4 pt,
  a navy) and a 1.2 pt line, because the one to the col crosses the palest sky.
- *Reference sky.* `/Stage /Reference` draws its sky as one smooth shading, not
  64 flat bands. `alpine_trail_ref.png` is not regenerated: it is the file the
  image model was sent, with the bands. The reference test still passes (0 px
  differ by more than 24; the largest difference is 3).
- *Poster QR caption.* 12 pt clear of the quiet zone (was 4).
- *Menu.* Dishes 15 pt (was 14), heads 17, margin 40 (was 64), so the longest
  dish keeps four leader dots (was one); the section gaps are tighter so the
  last Drinks row stays above the leaves (their edge is at x 323 there, read off
  the painting; R is 318). The QR caption moved under the code, clear of the
  pear.
- *Sizes.* "1055 × 1491" and the other pixel sizes use a ×, in the guide, the
  examples README and the sheets' captions (the sheets' facts font is Latin-1).

**Not done, and why.** *The menu's empty top-right corner:* it is paper. Filling
it takes new copy, which breaks the string-for-string match with the image
model's panel (24 strings), or moving the QR code there, which contradicts the
gallery manifest's alt text ("QR code in the lower left"); the manifest is held
by other PRs. *The sheets' "Panel=N" captions:* #1000.

**Open, owned elsewhere.** The three gallery stills (`hybrid_sky_talk.png`,
`hybrid_menu_card.png`, `hybrid_alpine_trail.png`) still show the old posters
until the manifest's owners re-render them (the sky_talk one differs only by its
QR caption: its sky is the painting, not the vector sky).
