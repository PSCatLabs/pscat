# Dry-media reference board (issue #233)

This is the reference board for epic #232 (drykit). Each entry is a real
photograph or scan, a short description of what it shows, and the
mechanism the observation implies for drykit. `docs/dev/design/DRYMEDIA.md` turns the
mechanisms into a design. The gallery tickets (#238, #240 and #242) compare
their renders against the crops below.

Rules followed:

- Every observation here was made by looking at the image. Nothing is
  described from memory. A trait in #233's list with no image behind it is
  marked **not yet sourced**.
- Crops are committed only for public-domain, CC0, CC BY and CC BY-SA
  sources. A CC BY-SA crop is a derivative of its source and stays CC
  BY-SA under the same attribution. A CC BY crop keeps its attribution.
  The licence of every file is in the tables.
- Measurements were read from the crops and are approximate (about
  ±20%). Perspective in some board photos inflates some of the ratios.

## Sources

| Crop in `refs/` | Source (Wikimedia Commons) | Author | Licence |
|---|---|---|---|
| `chalk_crypto_board`, `chalk_crypto_letters`, `chalk_crypto_arrow` | [Mathematics (cryptography) on a blackboard](https://commons.wikimedia.org/wiki/File:Mathematics_(cryptography)_on_a_blackboard.jpg) | David Malone | CC BY 4.0 |
| `chalk_einstein_fraction_bars` | [Einstein blackboard](https://commons.wikimedia.org/wiki/File:Einstein_blackboard.jpg) (Oxford, 1931, behind glass) | decltype | CC BY-SA 3.0 |
| `chalk_sphere_passes` | [Spherical-conic-chalkboard](https://commons.wikimedia.org/wiki/File:Spherical-conic-chalkboard.jpg) | Jacob Rus | CC BY-SA 4.0 |
| `chalk_ghosting_closeup` | [Hand pressed against a chalkboard revealing traces of past lessons](https://commons.wikimedia.org/wiki/File:Hand_pressed_against_a_chalkboard_revealing_traces_of_past_lessons_and_ideas_in_a_classroom_setting.jpg) | Shixart1985 | CC BY 2.0 |
| `chalk_eraser_swipes` | [Chalkboard Texture](https://commons.wikimedia.org/wiki/File:Chalkboard_Texture.jpg) | JOGOS Public Assets | CC BY-SA 4.0 |
| `chalk_full_board`, `chalk_ledge_erasers`, `chalk_double_underline` | [Math01](https://commons.wikimedia.org/wiki/File:Math01.jpg) | MathKnight | CC BY 2.5 |
| `chalk_fermi_circle` | [Enrico Fermi at the blackboard](https://commons.wikimedia.org/wiki/File:Enrico_Fermi_at_the_blackboard.jpg) (greyscale, cropped and reduced) | Smithsonian Institution | Public domain |
| `chalk_FAKE_font_composite` | [Black(green)board-eqns](https://commons.wikimedia.org/wiki/File:Black(green)board-eqns.jpg) | MikeRun, after Andrzej 22 | CC0 |
| `crayon_child_rhino_6y`, `crayon_fill_streaks`, `crayon_fill_ground` | [6 years old kid drawings of crayon drawings of rhinoceros](https://commons.wikimedia.org/wiki/File:6_years_old_kid_drawings_of_crayon_drawings_of_rhinoceros_in_the_Safari_Park.jpg) | MrAlanKoh | CC0 |
| `crayon_child_tadpole_3y` | [Child drawing age 3 crayon on paper](https://commons.wikimedia.org/wiki/File:Child_drawing_age_3_crayon_on_paper.jpg) | "My daughter Teresa" (uploader) | CC BY-SA 3.0 |
| `crayon_dense_fill` | [Purple glitter crayon markings](https://commons.wikimedia.org/wiki/File:Purple_glitter_crayon_markings.jpg) | Ser Amantio di Nicolao | CC BY-SA 4.0 |
| `crayon_tips` | [Crayons beside child coloring](https://commons.wikimedia.org/wiki/File:Crayons_beside_child_coloring_(Unsplash).jpg) | Aaron Burden | CC0 |
| `charcoal_student_portrait_1888`, `charcoal_stumped_tone`, `charcoal_hatching_on_tone` | [Portrait of a Man, Study from Life, 1888](https://commons.wikimedia.org/wiki/File:Drawing,_Portrait_of_a_Man-_Study_from_Life,_1888_(CH_18397817).jpg) (Cooper Hewitt; signed "3rd year") | Coralie E. Feilner | Public domain |
| `charcoal_cast_study_1878`, `charcoal_fine_hatching` | [Studietegning af klassisk gipsafstøbning af et hoved i profil, 1878](https://commons.wikimedia.org/wiki/File:Johannes_Taklo,_tilskrevet_-_Studietegning_af_klassisk_gipsafst%C3%B8bning_af_et_hoved_i_profil_-_1878.jpg) (an academy test drawing) | attributed to Johannes Taklo | Public domain |
| `charcoal_rubbed_ground` | [Akademitegning efter gipsafstøbning af Antinoos](https://commons.wikimedia.org/wiki/File:Vilhelm_Hammersh%C3%B8i_-_Akademitegning_efter_gipsafst%C3%B8bning_af_Antinoos.png) | Vilhelm Hammershøi | Public domain |
| `charcoal_stick_strokes` | [001 Charcoal 11 x 14 - Tom Bell](https://commons.wikimedia.org/wiki/File:001_Charcoal_11_x_14_-_Tom_Bell.jpg) | Tom Berlangero | CC BY-SA 4.0 |
| `charcoal_compressed_mass` | [Portrait study in charcoal](https://commons.wikimedia.org/wiki/File:Portrait_study_in_charcoal_by_Mary_MacKinnon.jpg) (a halftone reproduction) | Mary MacKinnon | Public domain |

The comparison panels in `prototypes/` (`chalk_compare.jpg`) contain crops from the cryptography board (CC BY 4.0) and the Einstein board (CC BY-SA 3.0), so those panels are CC BY-SA 3.0 with both attributions.

`chalk_compare_235.jpg` (#235) puts crops of five boards beside drychalk renders:
- the cryptography board (C1, CC BY 4.0, David Malone);
- the Einstein board (C3, CC BY-SA 3.0, decltype);
- the ghosting close-up (C5, CC BY 2.0, Shixart1985);
- the eraser swipes (C6, CC BY-SA 4.0, JOGOS Public Assets);
- the ledge (C9, CC BY 2.5, MathKnight).

It is CC BY-SA 4.0, with all five attributions.

`lecture_compare_238.png` (#238) puts the `gallery/chalkboard_lecture.ps` still above C7, C1, C11 (Fermi), C5 and C3, with 1:1 details of the render. It contains the C3 crop (CC BY-SA 3.0), so like `chalk_compare_235.jpg` it is CC BY-SA 4.0, with every attribution. It is built by `prototypes/lecture_compare.py`.

The crayon comparison panels (#239) are built by `prototypes/crayon_compare.py`. `crayon_compare.png` uses only the CC0 crops from MrAlanKoh's rhino drawing, so it stays CC0 apart from pscat's own renders. `crayon_compare_figure.png` contains `crayon_child_tadpole_3y`, so it is CC BY-SA 3.0, with the source's attribution.

The gallery piece's comparison panel (#240), `crayon_drawing_compare.jpg`, is built by `prototypes/crayon_drawing_compare.py` from Y5 only (the rhino drawing, CC0), so it is CC0 apart from pscat's own render. Y5 may be oil pastel rather than crayon (see its row), so the match of grain is approximate. `crayon_drawing_gs.png` is the same piece rendered by Ghostscript, whose `rand` lays the motions out slightly differently.
`lincoln_compare.png` (#242) puts the source photograph of the charcoal
portrait (`gallery/references/lincoln_berger_1864.jpg`: Anthony Berger,
Abraham Lincoln, 9 February 1864, Library of Congress, public domain)
beside the pscat render, over the 1888 student portrait, the 1878 cast
study and the H2 crop. All the sources are public domain, so the panel is
too, apart from pscat's own render. `prototypes/lincoln_compare.py`
builds it.

These sources were viewed but no crop is committed. One is not free to
reuse; the others add nothing the crops above lack:

- [DeSerres, "Drawing with an eraser and charcoal"](https://www.deserres.ca/blogs/all-articles/drawing-with-an-eraser-and-charcoal):
  photos of compressed charcoal laid over a sketchbook page, highlights
  lifted with a kneaded eraser, and white charcoal on top. Copyrighted, so
  only its observations are recorded.
- Commons: [Miami University chemistry lecture room, 1915](https://commons.wikimedia.org/wiki/File:Miami_University_chemistry_lecture_room_1915_(3191691912).jpg),
  [Charcoal sticks](https://commons.wikimedia.org/wiki/File:Charcoal_sticks_051907.jpg),
  [Charcoal pencils](https://commons.wikimedia.org/wiki/File:Charcoal_pencils_051907.jpg),
  [Kneaded eraser](https://commons.wikimedia.org/wiki/File:Kneaded_eraser.jpg),
  [Arthur Streeton, charcoal by Tom Roberts](https://commons.wikimedia.org/wiki/File:Arthur_Smike_Streeton_1890_charcoal_drawing_by_Tom_Roberts_AGNSW_3269.jpg).
  All are public domain.

## Chalk

### Observations

| # | Crop | What it shows | Mechanism for drykit |
|---|---|---|---|
| C1 | `chalk_crypto_letters` | The writing is thin, with strokes about 1/20 of cap height. It isn't pure white: lines read as a grey-white about 70–85% of the way from board to white, and brighter where pressed. | Chalk colour is a lerp toward white, not white. Pressure changes brightness more than width. |
| C2 | `chalk_crypto_arrow` | A long ruled stroke is bright at its start and dimmer, almost broken, along much of its length. The arrowhead is pressed brighter. | Pressure (`/Touch`) varies along a stroke. A landing (heavier start) and a tired middle are the common profile. |
| C3 | `chalk_einstein_fraction_bars` | Fraction bars drawn with a worn, flat stick show two bright parallel edges with a granular, dimmer middle. Edges are lumpy, with specks just beyond them. Stems taper and dim where the chalk lifts. | The contact patch of a worn stick is not uniform: its two edges press harder than its face. Fine tooth break-up shows mainly at edges and in light-touch spans. |
| C4 | `chalk_sphere_passes` | Long lines are two or three nearly parallel passes, slightly diverging. Inside a pass the deposit is **streaked along the travel direction**, with dark gaps that run with the stroke rather than round pits. | Tooth catch is anisotropic: a skipped valley becomes a streak along travel, because the stick bridges it and lands further on. |
| C5 | `chalk_ghosting_closeup` | Erased writing survives as low-contrast lines on a cloudy haze of dust. Wiped patches (finger, cloth) are *darker* than the surrounding haze, because they removed the dust film. The board's own surface has a fine leathery grain where the chalk breaks up. | Ghosting is old strokes at much reduced strength, plus a haze layer. Wiping removes haze, so a wipe draws in board colour over the haze. |
| C6 | `chalk_eraser_swipes` | A heavily erased green board: overlapping curved swipe bands about one eraser wide, each streaked along its own arc (felt fibres), plus scattered bright crumbs and a stamped rectangular eraser imprint. | Eraser swipes are wide arcs with internal streaks along the arc: a broad, low-strength stroke whose tooth streaks follow its own direction. |
| C7 | `chalk_full_board` | A full lecture board in two sliding panels. Lines are about 1.6–2× cap height apart. Text is roughly upright with drifting baselines, plus double underlines, arrows, small diagrams and vertical dividers. Two felt erasers sit on the ledge. | Board layout data for #237/#238. Cap height is about 1/11 of a panel's height. Stroke/cap is about 1/13 here, with a thicker stick than C1. |
| C8 | `chalk_double_underline` | A "double underline" is two separate strokes, not parallel. The second is shorter and converges on the first. | An underline is two strokes with independent jitter. Never offset copies. |
| C9 | `chalk_ledge_erasers` | Erasers on the tray sit in a light halo of fallen dust. The tray lip reads lighter than the wall below it. | Ledge dust is a fallout deposit concentrated at the tray, densest around erasers. |
| C10 | `chalk_FAKE_font_composite` | **What a fake looks like:** a TeX font pasted onto a board photo. Strokes are uniform, perfectly white and perfectly smooth, with typeset spacing and identical repeated glyphs. | The negative example for #238. Every trait above is missing here. |
| C11 | `chalk_fermi_circle` | Fermi's board (1940s press photo): a diagram drawn freehand at lecture scale. The circles are slightly out of round and their strokes are heavy and even. The formulas above it are written larger than the diagram's labels. | Diagrams are drawn bigger and bolder than the writing. A freehand circle is one long stroke whose closure overshoots. Used for #238's unit circle. |

### Board numbers (for #236, #237, #238)

| Quantity | crypto (C1) | Einstein (C3) | Math01 (C7) |
|---|---|---|---|
| stroke width / cap height | ~1/20 | ~1/15 | ~1/13 |
| cap height / board (or panel) height | ~1/12 | ~1/9 | ~1/11 |
| line pitch / cap height | ~1.7 | ~1.9 | ~1.6–2 |
| slant | 0 to +5° | about +5° | about 0° |
| baseline drift | follows the arrow it annotates, up to ~10° | a few degrees per line | small |

### #233 chalk traits

| Trait in #233 | Status |
|---|---|
| dust halo and fallout below strokes | **Partly sourced.** C5 and C9 show haze and ledge dust. No crop shows a dust halo directly under fresh strokes. |
| broken edges where the chalk skips the grain | C3, C4 |
| wide marks from the side of the stick | C3 (flat-stick bars), C6 (eraser-width swipes). No crop shows a deliberately side-drawn stroke. |
| heavier landing dots and stroke starts | C2, C3 |
| ghosting from erased lectures, eraser swipe marks | C5, C6 |
| professor's writing: size, slant, drifting baselines | C1, C7 and the table above |
| boxed results | **Not yet sourced.** C7 has double underlines and C8 their construction, but no box. |
| arrows | C2, C7 |

## Crayon

### Observations

| # | Crop | What it shows | Mechanism for drykit |
|---|---|---|---|
| Y1 | `crayon_fill_streaks` | A 6-year-old's grey fill: white gaps are **elongated along the scribble direction** (mostly horizontal), irregular, clumped, roughly 15–30% of a lightly pressed area. The wax lies in streaks. | The same anisotropic catch as C4: holes streak along travel. A crayon's rigid tip bridges paper valleys. |
| Y2 | `crayon_fill_ground` | The pink and brown fills are horizontal back-and-forth scribble with ragged band edges and white seams between bands. Heavily pressed black (the feet) is almost solid, with a few specks. | Pressure moves coverage from speckled to solid: the touch threshold. Scribble fill leaves gaps between passes as well as within them. |
| Y3 | `crayon_dense_fill` | A dense adult fill: fine parallel striations along each curved stroke (the wax edge dragging), darker patches where passes overlap, and paler blotchy islands where the paper's low spots stayed bare. | Two scales at once: fine striation along travel (a tool property) and blotches at the paper's mottle scale (a surface property). Overlap builds up darker. |
| Y4 | `crayon_child_tadpole_3y` | A 3-year-old's "tadpole" figure: arms and legs grow from the head, with hair as a row of short strokes. Lines are darker where the stroke starts (the left arm's pressed blob at the head) and fade toward the end. | The child-drawing vocabulary and pressure profile for #239/#240: press on landing, fade out. |
| Y5 | `crayon_child_rhino_6y` | A 6-year-old's whole picture. A tree trunk on the left, with its canopy as a band of green scribble across the top; a large yellow sun disc behind the subject; the rhino in the middle; ground bands at the bottom. Colours are laid side by side with white seams, and outlines are drawn over the fills. The Commons title says crayon, but the waxy, dense fill could be oil pastel. | Composition for #240: bands at top and bottom, the subject in the middle, outlines last. |
| Y6 | `crayon_tips` | New crayons have conical tips with a small flat. Worn ones have an oblique flat face. | Tip wear: a point that widens into a flat face, so the contact width grows over use. |

### #233 crayon traits

| Trait in #233 | Status |
|---|---|
| waxy coverage that skips the paper grain | Y1, Y2, Y3 |
| wax buildup where a stroke turns | **Partly sourced.** Y3 shows darker build-up where passes overlap. No crop isolates a turn. |
| one colour layered over another | **Not yet sourced.** Y5 has colours side by side and outlines drawn over fills, but no true colour-over-colour layering. |
| back-and-forth scribble fill that goes outside the lines | Y2, Y5 |
| children's wobbly, overshooting lines and typical subjects | Y4, Y5 |
| the sky as a band at the top (#240) | **Not yet sourced.** Y5's top band is a tree canopy, not sky. |

## Charcoal

### Observations

| # | Crop | What it shows | Mechanism for drykit |
|---|---|---|---|
| H1 | `charcoal_stumped_tone` | Stumped (blended) tone. The grey is never flat: an isotropic felt-like grain, and on the right fine **vertical lines** in the paper, consistent with laid paper. The museum record doesn't state the support, so "laid" is inferred from the look. | The paper's surface can have two components: a random grain, plus a directional line pattern, as laid drawing papers such as Ingres have. |
| H2 | `charcoal_hatching_on_tone` | Light hatching beside the head: thin parallel diagonal lines that break into **rows of dots and dashes** where they cross the tooth. Next to them, dense tone has a granular "fur". | At light touch only the tooth's peaks catch, so a thin line becomes a dotted line. This is the shared catch at low `/Touch`. |
| H3 | `charcoal_student_portrait_1888` | A third-year student's portrait study, "signed in charcoal" per the Commons record: stumped tones, diagonal hatching in the background, a rubbed vignette edge, and bright, hard-edged whites on the beard. The record gives no medium line. The whites look applied (white chalk or heightening) rather than lifted, because they are brighter than the bare paper around the head. | Portrait structure for #242: block-in tone, smudge, hatch, then highlights. Applied whites are a pigment stroke on top. A lift (H8) is paper colour through the peaks. |
| H4 | `charcoal_fine_hatching` | Fine short hatching at about 45° over a stumped base. Paper-white points of tooth survive even in the darks. | Even heavy tone keeps the highest-peak specks, unless it is smudged into the valleys. Smudging is the step that fills them. |
| H5 | `charcoal_rubbed_ground` | A background rubbed with stump or cloth. Directional swipe marks and darker streaks lie over an evenly speckled tooth. | A smudge is a directional, low-contrast stroke: it spreads tone along its direction and raises valley fill. |
| H6 | `charcoal_stick_strokes` | Stick-charcoal contour strokes on smooth paper: square-ended dashes from a chisel face, darker landings, and flicked tails that split into bristle-like streaks. A faint grey haze over the page comes from the hand dragging dust. | A stick's flat end gives square caps. Lift-off splits into streaks. Dust haze (fallout and smear) tones the paper itself. |
| H7 | `charcoal_compressed_mass` | Compressed charcoal masses with directional strokes, and lights left as the paper or lifted out as white streaks. | Dark compressed charcoal behaves as a near-solid deposit, with lifts as shaped light strokes. |
| H8 | DeSerres (not committed) | Kneaded-eraser lifts on compressed charcoal are grey, soft-edged and speckled, never back to paper white. White charcoal laid on top reads brighter and harder. | A lift removes pigment from the tooth's peaks first. It is paper colour drawn *through the catch* at the lift's pressure, so the valleys stay dark. |

### #233 charcoal traits

| Trait in #233 | Status |
|---|---|
| vine, compressed and pencil charcoal; hard vs soft grades | **Partly sourced.** H6 (stick), H7 (compressed) and H2/H4 (pencil-like hatching). There is no labelled side-by-side of grades. |
| hatching and cross-hatching | H2 and H4 show hatching. **Cross-hatching is not yet sourced.** |
| smudging with stump or finger | H1, H5 |
| highlights lifted with a kneaded eraser | H8 (observed, not committed) |
| faint construction lines | **Not yet sourced.** |
| how student portrait studies look | H3, plus the 1878 academy cast study (`charcoal_cast_study_1878`) |
| charcoal on newsprint | **Not yet sourced.** |

## Gaps to fill before the gallery tickets

These are needed before #238, #240 and #242 compare renders. Each can be
found by the gallery ticket's own agent:

- a chalk box around a result, and a dust halo under fresh strokes (still not sourced after #238: no free photo of a boxed result turned up in a Commons search, so #238's box follows mathkit's `mkframe`);
- crayon colour-over-colour layering, build-up at a turn, and a sky band;
- charcoal cross-hatching, construction lines, and charcoal on newsprint.
