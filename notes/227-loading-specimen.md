## Paint loading L5b: the specimen, the gs driver and the catalog check (issue #227, 2026-10-13)

Slice L5b of #183's design (`docs/PAINT_LOADING.md` 6 and 7). Every loading slice
(L1-L5) is merged, so this slice adds no rendering code: `lib/` is untouched, and
byte identity holds by construction (`scripts/identity_check.sh` needs a `--control`
that uses a key only the branch has, and this branch adds none, so it was not run).

- **`examples/paintkit_loading_demo.ps`** (780x640, committed render at
  `docs/paintkit_loading_demo.png`, 2x supersampled like the gallery stills, laid out
  as dark captioned panels like the canvas specimen). Top row: the same evergreen
  twice, brush-relative and then `/Light 150`; a knife snow pull loaded white and
  blue. Bottom row: a broad brush loaded ochre and teal at `/ColorMix` 0, 0.3 and 1;
  two bushes and bank lips, the lower with a warm `/HighlightColor` and a cool
  `/ShadowColor`; and the loaded evergreen on a shore with its reflection pulled down
  by a directional `pkwet`. About 15 s to render.
- **Choices made by looking at the render.**
  - The branches are real `moveto`/`lineto` strokes from the trunk outward, never
    pressed fans, because `/Light` flips by chord and a chord under `/Pitch` is not
    flipped. The brush-relative tree is the trap the design describes (section 2.3):
    its left branches carry the lit colour underneath and the right ones on top.
  - `/ColorMix 0.12` and wide branches (`Width` 0.17 of the tree's height) keep both
    colours legible at page size; the first attempt at 0.3 on narrow tiers blended to
    one green. `/Light 150` rather than 120 so the lit face reads as the sun's side.
  - The tree stands on a strip of bank above the waterline. Standing it on the
    waterline made its lowest branch meet its own mirror image as a diamond
    (alpine_lake's note about the same chevron).
  - The pull uses a 70pt tree and a 135pt waterline so the pulled fringe has room to
    fade below the reflection; with a tall tree the fade was hidden under the core
    pass. Mirror and `srand` live inside the pkwet procedure, the trunk takes the
    inherited colour, and nothing in it calls `setrgbcolor`.
  - The bushes are stacked, not side by side, so each is wide enough to show the warm
    highlight against the plain one's pale grey-green.
- **Test 12** `ghostscript_accepts_paint_loading`: one driver with every new key on all
  five adopters inside a directional `pkwet`, under `true setpacking`. It runs under
  pscat first (and asserts ink), so the test proves something where `gs` is absent.
  Mutation-checked: `/Colors 5` in the driver fails it. `ghostscript_accepts_the_loading_demo`
  runs the specimen file under gs.
- **`the_loading_demo_shows_what_each_panel_is_for`**: panel pixels, not a hash, so a
  re-tuned page does not need a re-pin: both trees paint and `/Light` moves over 1500
  of their pixels (the panels are otherwise identical, which needed a threshold: they
  differ by antialiasing alone at a 250pt offset), white and blue in the pull, ochre
  and teal in the broad strokes, warm pixels on the hued bush and none on the plain
  one, and the reflection's leaf-green pixels at the foot a third or less of the band
  under the bank. Mutation-checked: `/Light` off and `/HighlightColor` removed each fail.
- **Catalog check** `the_catalog_names_the_loaded_color_keys_on_every_adopter`
  (`tests/capabilities.rs`): `/Colors`, `/ColorMix`, `/Light`, `/HighlightColor` and
  `/ShadowColor` appear with their design units on each preset that takes them, no
  British spelling is advertised, and `pkwet`'s `/Direction` text says loaded colors
  are graded. It passed on the first run, so no `lib/` text needed editing. The
  `@summary` lines of `pkfan`, `pkbroad` and `pktrowel` do not mention the load (only
  `pkdab`'s does); their `@param` rows do, which is where `--capabilities` shows it.
  Filed as #291 rather than edited here, so this PR stays free of `lib/` changes.
- **Docs.** A README paragraph with the render, a psart bullet (brush-relative load,
  `/Light` and real strokes, narrow dark half, hue keys, no `setrgbcolor` inside
  pkwet) and HANDOFF's "specimen is next" reworded.
- **The reflection is the same tree.** The sharp tree and the pkwet procedure seed once
  and draw the trunk then the branches, in the same order as `ldtree`, so the reflection
  mirrors the tree's bristles instead of being a second random tree; `/ldlight` is set
  explicitly in the shore panel rather than inherited from the previous panel. The fade
  assertion was mutation-checked too: `/Pickup 0` on the pull makes it fail, so it
  measures `pkwet` grading the loaded colours and not just the reflection's geometry.
- Not here: `/TipColor` and `/TipReach` (L6, #229), `pkdry` and `pkoil` ridges (L7,
  #230), the `@summary` lines (#291), and converting `gallery/alpine_lake.ps` to
  loaded brushes (the second specimen, #187). No site card: the render is committed
  under `docs/`.
