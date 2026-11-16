# 858: regenerate the committed renders before 0.7.0

Mechanical regeneration with a fresh release build of `main` at `cbbf8fe` (0.7.0-rc.5). No art, source
or library changes: only PNGs, and this note.

## What was regenerated, and how

- **Gallery stills** (`gallery/renders/*.png`, 47 in the manifest): each piece rendered with
  `pscat --page WxH --dpi 144 --png` (the 2x still; `gallery/README.md`, "Re-rendering the stills"),
  from the repo root. Three exceptions follow the committed file, not the default:
  - `hortus` stays 1x (620 x 800, the committed size; `--page 620x800`, 72 dpi). It is the one still that
    is not 2x: a follow-up, not changed here.
  - `keepin_it_real` was committed with the README's `2 2 scale` prelude and a doubled `--page`. `--dpi 144`
    gives a different picture for this piece alone (max 85 levels against 36), so the prelude is used.
  - `pscat_card` uses `/CardProof` at 300 dpi (its header); it came out byte-identical.
  For every other piece the `--dpi 144` and prelude methods give identical pixels.
- **Other committed renders the site or README show**, each with its header or guide command:
  `art/logo/renders/pscat-logo-d-{512,48}.png`, `docs/dev/design/drymedia/chalk_cli_demo.png` (the
  `--seed 7` integral from `docs/guides/dry-media.md`), `lib/styles/specimen_{steampunk,psychedelic,scifi,toon}.png`,
  `lib/fonts/specimen.png` (108 dpi: the committed size, 918 x 1188, is 1.5x; the header's command is 72 dpi),
  `lib/fonts/specimen2.png`, `fonts/catalog/specimen-{1,2}.png`, and the five `docs/guides/hybrid/*` sheets
  (`pscat sheet` commands in `examples/hybrid/compare_*.ps`).
- **Byte-identical, left untouched:** 10 gallery stills (`varsity_backs`, `four_readings`, `ring_unrolled`,
  `blueprint_rose`, `family_tree`, `painters_city`, `mirror_garden`, `mandelbulb`, `dreaming_network`,
  `mochi_in_oils`), `pscat_card`, `art/brand/renders/{social-preview,certificate}.png` and the
  `ferry`, `menu_card`, `alpine_trail` compare/process sheets (already re-rendered after #1145).
- Not regenerated: `plugin/.claude-plugin/icon.png` (the plugin icon, same program as the logo, not shown by
  the site or README) and the other images under `docs/` that no manifest entry or README shows.

Every render exited 0 with nothing on stderr but the "wrote" line.

## Classification

Pixels were compared decoded (RGBA, same size). 48 files changed: **36 edge-level, 5 edge-located but strong
(grain), 7 larger**. Crops of three of the five strong ones are in
`docs/dev/review-log/pending/858-regenerate-gallery-renders/`.

To find the cause, the changed pieces were also rendered with binaries built at `56006db` (just before
#778), `946290c` (#778) and `bbb9037` (#852), all with this tree's `lib/`:

- The committed still equals the pre-#778 binary's render (same pixels) for 30 of the 36 changed gallery
  stills. The other six: the three hybrids (#1145, below) and `flying_white_reeds`, `mochi_denim_blue`
  and `evening_stream`, which differ by 2 to 13 levels on under 0.1% of pixels from small library edits
  since they were committed.
- **#778** (exact-area coverage, #692) is the whole of the edge change: up to 50 levels on anti-aliased
  edges and glyphs. The strongest shifts (84 to 137 levels, on under 1% of pixels) are in the five grain-heavy
  pieces `crayon_drawing`, `lincoln_charcoal`, `fugitive_pigments`, `flying_white_reeds` and `alpine_lake`:
  thousands of sub-pixel shapes that now keep their gray. The largest connected region of such a change is a
  dozen pixels.
- **#852** (f32 compositing, #731 part 1) adds a small flat tone shift (at most 18 levels, in
  `evening_stream`; 14 or under elsewhere) on pieces that stack faint layers. It is the only change in
  `first_rain` that is not an edge: 91% of its pixels move, by 13 levels at most (mean +0.2), with no
  visible effect.
- Everything after #852 (limits PRs, #1040 shfill `/Extend`, #1215, #1203, #1047) changes no still beyond
  `varsity_backs` (already re-rendered by #1040); the current binary renders the same pixels as the
  `bbb9037` one, except `released_memo`, which differs on 0.0% of pixels by 19 levels at most, on glyph
  edges. That change comes from a PR after #852 and was not traced to one.

### Larger than edge-level (7), each explained

| file | what changed | caused by |
|---|---|---|
| `gallery/renders/hybrid_menu_card.png` | menu type sizes, margins, the QR caption | #1145 (#1029, #1042) |
| `gallery/renders/hybrid_alpine_trail.png` | the dashes follow the photographed route above the hut; the leader lines have a wider dark halo | #1145 |
| `gallery/renders/hybrid_sky_talk.png` | the QR caption moved 12 px | #1145 |
| `docs/guides/hybrid/sky_talk_compare.png` | the same caption in panel (c) | #1145 |
| `gallery/renders/first_rain.png` | flat tone shift, 13 levels at most, mean +0.2 | #852 |
| `fonts/catalog/specimen-2.png` | page 2 of the catalog now lists the Korean, Japanese and Thai faces and the two Korean ones | the specimen program grew in Stage 22 and 23 (July); the still dates from Stage 18 |
| `docs/dev/design/drymedia/chalk_cli_demo.png` | the faint ghost writing sits in different places | #698 (PR #812), merged two days after the demo was rendered |

The three gallery hybrids are rendered from the same programs as panel (c) of the compare sheets, which
were already re-rendered after #1145.

### Sizes

The 48 files grow from 32.2 MB to 32.8 MB in total (+1.9%). The largest growth, from sub-pixel shapes
keeping their gray (less compressible): `fonts/catalog/specimen-2.png` (83 -> 149 KB, more content),
`specimen-1.png` (55 -> 93 KB), `compositors_proof` (153 -> 251 KB, 1.64x), `ring_of_type` (517 -> 745 KB),
`out_of_register` (1.32 -> 1.89 MB), `fugitive_pigments` (412 -> 580 KB). Fifteen files shrink
(`chalkboard_lecture` by 16%, `crayon_drawing` by 24%, `lincoln_charcoal` by 10%).

## Per-file table (gallery stills)

"via #778" and "via #852" are the largest level change that PR alone makes to the piece (with this tree's lib).

| still | class | px changed | max level | via #778 | via #852 | size before -> after |
|---|---|---|---|---|---|---|
| golden_bloom | edge-level | 8.6% | 21 | 22 | 4 | 833 -> 970 KB |
| cathedral_rose | edge-level | 20.6% | 4 | 0 | 4 | 1929 -> 1885 KB |
| ember_tree | edge-level | 4.2% | 37 | 36 | 5 | 402 -> 409 KB |
| fern | edge-level | 8.4% | 39 | 50 | 14 | 704 -> 725 KB |
| silk_waves | edge-level | 5.4% | 2 | 0 | 2 | 240 -> 238 KB |
| frost_mandala | edge-level | 9.9% | 14 | 11 | 6 | 1014 -> 947 KB |
| ring_of_type | edge-level | 6.6% | 43 | 43 | 2 | 504 -> 727 KB |
| hundred_lines | edge-level | 1.3% | 2 | 0 | 2 | 120 -> 120 KB |
| hortus | edge-level | 6.3% | 39 | 40 | 7 | 99 -> 108 KB |
| woven_labyrinth | edge-level | 5.3% | 35 | 35 | 2 | 64 -> 74 KB |
| infinite_descent | edge-level | 2.5% | 44 | 44 | 6 | 216 -> 224 KB |
| recursive_peaks | edge-level | 1.9% | 39 | 39 | 4 | 81 -> 94 KB |
| ripple_range | edge-level | 3.7% | 30 | 30 | 2 | 213 -> 243 KB |
| field_notes | edge-level | 1.0% | 38 | 37 | 3 | 68 -> 72 KB |
| compositors_proof | edge-level | 3.4% | 42 | 43 | 2 | 149 -> 245 KB |
| lodestone | edge-level | 4.0% | 28 | 28 | 3 | 358 -> 359 KB |
| plum_branch | edge-level | 0.8% | 30 | 29 | 3 | 86 -> 116 KB |
| flying_white_reeds | edge-level (grain, strong) | 7.5% | 101 | 101 | 7 | 191 -> 212 KB |
| fugitive_pigments | edge-level (grain, strong) | 5.5% | 137 | 137 | 4 | 402 -> 566 KB |
| mochi_denim_blue | edge-level | 10.8% | 39 | 40 | 6 | 1037 -> 1098 KB |
| first_rain | tone: #852 (f32 compositing), mean +0.2, max 13 | 91.3% | 13 | 14 | 8 | 523 -> 534 KB |
| firefly_census | edge-level | 6.3% | 24 | 25 | 4 | 447 -> 464 KB |
| basin_and_range | edge-level | 1.9% | 28 | 28 | 2 | 729 -> 731 KB |
| out_of_register | edge-level | 17.1% | 34 | 34 | 4 | 1286 -> 1844 KB |
| nightfall_triptych | edge-level | 8.5% | 20 | 21 | 3 | 321 -> 305 KB |
| keepin_it_real | edge-level | 0.9% | 36 | 36 | 1 | 838 -> 848 KB |
| alpine_lake | edge-level (grain, strong) | 38.0% | 83 | 84 | 14 | 1709 -> 1699 KB |
| chalkboard_lecture | edge-level | 16.9% | 44 | 45 | 9 | 2263 -> 1896 KB |
| crayon_drawing | edge-level (grain, strong) | 18.9% | 121 | 120 | 6 | 1071 -> 818 KB |
| lincoln_charcoal | edge-level (grain, strong) | 30.4% | 86 | 86 | 6 | 1156 -> 1045 KB |
| evening_stream | edge-level | 25.5% | 45 | 46 | 18 | 1497 -> 1478 KB |
| released_memo | edge-level | 8.9% | 33 | 33 | 15 | 808 -> 789 KB |
| time_traveler_memo | edge-level | 14.3% | 30 | 30 | 14 | 1121 -> 1066 KB |
| hybrid_sky_talk | larger: #1145 (caption moved) | 0.2% | 226 | 72 | 3 | 2206 -> 2207 KB |
| hybrid_menu_card | larger: #1145 (type sizes, margins, QR caption) | 7.1% | 224 | 91 | 2 | 2196 -> 2209 KB |
| hybrid_alpine_trail | larger: #1145 (route traced from photo, leader halos) | 0.4% | 231 | 43 | 3 | 2274 -> 2272 KB |
