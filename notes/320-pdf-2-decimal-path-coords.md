# #320: pdf export, 2-decimal path coordinates

The maintainer decided (comment on #320) to write path coordinates in PDF
content streams with two decimals instead of three. #254 measured the lever
(notes/254-pdf-drop-degenerate-lineto.md); this is the shipped version.

## What shipped

- `path_coord()` in src/pdf.rs (two decimals) is used by `PsPath::to_pdf_ops`
  (src/gfx.rs; the function lives there, so touching gfx.rs was unavoidable)
  for every `m l c` operand, in fills, strokes **and clips** (clips share
  `path_ops`).
- `fmt_num(v, decimals)` trims trailing zeros and the bare `.`, and never
  prints `-0` (a tiny negative becomes `0`). `f()` is `fmt_num(v, 3)`; it had
  the same latent `-0` problem at three decimals (e.g. `-0.0004`).

## What keeps three decimals, and why

| number | decimals | why |
|---|---|---|
| colours (`rg`/`RG`), alpha (`ca`/`CA`) | 3 | a channel step is 1/255 = 0.004; two decimals would band |
| line width (`w`), miter limit | 3 | hairlines (0.125, 0.25 are common) lose up to 4% at two decimals; 0.004 would vanish |
| dash array and phase | 3 | same reasoning, and dash lengths are often under 1 pt |
| image `cm` (`sx ky kx sy tx ty`) | 3 | `sx`/`sy` are the image's full device extent, so a 0.005 error moves its far edge by 0.005 pt at most; but there is one `cm` per image, so two decimals would save nothing and three costs nothing |
| page flip `1 0 0 -1 0 H cm` | integer | `H` is an integer |

Path coordinates are device-space points at the page's own scale, so a
0.005 pt rounding error is at most 0.01 px at 144 dpi. Rounding is a
pure function of the coordinate, so two shapes that share an edge still print
the same text for it, and a fill and the clip around it stay consistent.

## Degenerate segments (the #254 closepath lesson)

`to_pdf_ops` already judged equality on the *printed* coordinates; with two
decimals that matters more, because short real segments now collapse onto the
current point. It tracks both the printed and the raw (unrounded) current
point, reset to the subpath start by `closepath`.

- A `l` that prints equal to the current point and was a **genuine**
  zero-length segment in the program: unchanged from #254 (dropped in a fill
  or clip when more geometry follows; kept in a stroke, where it decides caps
  and joins).
- A `l` that prints equal only because of **rounding**: dropped in fills,
  clips *and strokes* whenever the subpath has already written a segment or
  more `l`/`c` follows. It would otherwise be a zero-length segment the
  program never had. Kept only when it is all there is of its subpath
  (`m` + `l`, or `m` + `l` + `h`), so a tiny dash still prints as a dot with
  round caps. Of a run of collapsed linetos exactly one remains.
- Known effect of rounding itself (not of the drop rule): the PDF spec paints
  a degenerate subpath (all points coincident) only with round caps. A lone
  dash shorter than about 0.005 pt with butt or square caps used to print as a
  tiny rectangle and now prints nothing.
- `c` is never dropped: a speck's quarter-circle control points stay distinct
  at two decimals (radius 0.17 pt), and a curve whose endpoints coincide is a
  legitimate loop.
- After `closepath` the current point is the subpath start, so a following
  line back to the last pre-close point is a real segment (unchanged, tested).

Tests: `path_coordinates_use_two_decimals_and_other_numbers_keep_three`,
`rounding_collapsed_linetos_are_dropped_in_strokes_unless_alone`
(tests/pdf.rs) and `number_formatting_trims_and_never_prints_negative_zero`
(src/pdf.rs). No existing PDF test asserted coordinate text beyond integers,
so none was changed.

## Measurement

Baseline binary: detached worktree of origin/main (110ac7a, already includes
#254's `l` drop) with its own target dir; branch binary built from this
commit. Both ran in the branch worktree over the same 103 tracked
`examples/*.ps` + `gallery/*.ps` (so the user's untracked gallery files never
join in), `--pdf`, with `--page` from `%%BoundingBox`. Each PDF was rasterised
with `gs -sDEVICE=png16m` at 72 dpi (examples) and 144 dpi (gallery), as
scripts/identity_check.sh, and compared by 10x10 block means (the metric of
tests/golden.rs, bar 6.0). All 103 files exported and rasterised with exit 0 and
equal page counts.

- Total PDF size: **240,108,452 -> 192,651,226 bytes (-19.8%)**. Per-file
  change: median -13.8%, from -23.3% to -0.4%.
- alpine_lake: 72,365,476 -> 58,707,962 (-18.9%; the issue's 74.4 -> 58.8 MB
  includes #254's -2.2%).
- gs rasters differ in **102 of 103** files at the pixel level (gs draws
  without anti-aliasing, so a 0.005 pt shift flips edge pixels). Differing
  pixels: median 70, worst alpine_lake 4,942.
- Mean block difference (golden metric, bar 6.0): **worst 0.197
  (examples/printkit.ps), median 0.012**.
- Why 0.197 here against the 0.10 quoted in #254: a variant with only
  `{v:.3}` -> `{v:.2}` in `to_pdf_ops` (no collapsed-`l` rule), run on
  printkit, template_poster, fern and drychalk_specimen, gives the same pixel
  counts and block diffs to four digits (printkit 0.1972 both ways). So the
  stroke drop rule moves nothing measurable; the difference from #254's figure
  is the metric or corpus (this run uses the tests/golden.rs mean over all
  blocks, per file, worst over pages).
- Largest single 10x10 block-mean channel difference: worst 23.2
  (examples/template_poster.ps), median 4.7. That is one row of ten pixels
  flipping between ink and paper in an unantialiased raster (10% of 255), not
  a shape change; it is far from the per-file mean the golden test judges.

| file | before (bytes) | after (bytes) | size | differing px | mean block diff | max block diff |
|---|---:|---:|---:|---:|---:|---:|
| gallery/alpine_lake.ps | 72,365,476 | 58,707,962 | -18.9% | 4,942 | 0.0630 | 3.87 |
| examples/drychalk_specimen.ps | 31,147,227 | 24,448,496 | -21.5% | 324 | 0.0151 | 11.55 |
| examples/drycrayon_specimen.ps | 28,707,709 | 23,018,864 | -19.8% | 173 | 0.0252 | 13.58 |
| examples/drykit_specimen.ps | 26,204,997 | 20,329,523 | -22.4% | 53 | 0.0046 | 1.71 |
| examples/paintkit_wet_pull_demo.ps | 25,084,233 | 19,250,375 | -23.3% | 400 | 0.0117 | 3.06 |
| examples/drycrayon_motion_demo.ps | 15,884,492 | 12,692,946 | -20.1% | 62 | 0.0124 | 4.74 |
| examples/paintkit_loading_demo.ps | 9,283,277 | 7,225,458 | -22.2% | 224 | 0.0115 | 7.65 |
| examples/paintkit_canvas_demo.ps | 3,955,411 | 3,382,417 | -14.5% | 163 | 0.0272 | 13.44 |
| gallery/fern.ps | 3,304,493 | 2,940,860 | -11.0% | 1,597 | 0.0327 | 6.19 |
| examples/paintkit_wet_demo.ps | 2,133,203 | 1,846,940 | -13.4% | 258 | 0.0331 | 5.60 |
| gallery/fugitive_pigments.ps | 1,773,314 | 1,533,258 | -13.5% | 664 | 0.0395 | 6.96 |
| gallery/first_rain.ps | 1,750,816 | 1,516,850 | -13.4% | 2,232 | 0.0051 | 1.06 |
| gallery/mochi_denim_blue.ps | 1,742,920 | 1,406,463 | -19.3% | 1,488 | 0.0123 | 2.86 |
| gallery/out_of_register.ps | 1,513,730 | 1,323,768 | -12.5% | 2,131 | 0.0975 | 14.91 |
| examples/paintkit_spray_demo.ps | 1,370,244 | 1,179,738 | -13.9% | 80 | 0.0187 | 4.44 |
| examples/paintkit_fan_demo.ps | 1,111,146 | 864,758 | -22.2% | 182 | 0.0332 | 4.20 |
| examples/paintkit_motion_demo.ps | 1,030,547 | 810,468 | -21.4% | 181 | 0.0182 | 2.16 |
| gallery/flying_white_reeds.ps | 784,243 | 641,043 | -18.3% | 380 | 0.0160 | 6.81 |
| examples/font_catalog.ps | 675,918 | 566,654 | -16.2% | 129 | 0.0320 | 8.10 |
| examples/fillkit.ps | 646,177 | 633,382 | -2.0% | 46 | 0.0099 | 6.66 |
| examples/paintkit_wash_demo.ps | 558,646 | 477,323 | -14.6% | 258 | 0.0118 | 4.29 |
| examples/halftone.ps | 501,159 | 453,942 | -9.4% | 150 | 0.0667 | 6.90 |
| gallery/firefly_census.ps | 493,582 | 431,280 | -12.6% | 734 | 0.0101 | 4.14 |
| examples/paintkit_dry_demo.ps | 448,405 | 394,966 | -11.9% | 296 | 0.0059 | 3.45 |
| examples/paintkit_dab_demo.ps | 387,446 | 317,453 | -18.1% | 103 | 0.0257 | 5.60 |
| gallery/keepin_it_real.ps | 323,620 | 316,837 | -2.1% | 239 | 0.0119 | 18.76 |
| examples/strokeglyphs.ps | 318,078 | 263,424 | -17.2% | 239 | 0.0311 | 9.20 |
| examples/paintkit_broad_demo.ps | 303,781 | 241,017 | -20.7% | 178 | 0.0316 | 5.60 |
| examples/paintkit_trowel_demo.ps | 292,771 | 236,373 | -19.3% | 193 | 0.0327 | 4.60 |
| examples/stippling.ps | 284,650 | 248,938 | -12.5% | 139 | 0.0826 | 23.00 |
| examples/style_steampunk.ps | 281,179 | 224,356 | -20.2% | 43 | 0.0124 | 3.26 |
| examples/surfacekit.ps | 269,486 | 239,514 | -11.1% | 59 | 0.0081 | 9.20 |
| gallery/ember_tree.ps | 264,296 | 219,212 | -17.1% | 450 | 0.0060 | 2.85 |
| gallery/golden_bloom.ps | 240,940 | 200,483 | -16.8% | 586 | 0.0160 | 2.26 |
| examples/lettering.ps | 206,868 | 177,700 | -14.1% | 102 | 0.0174 | 7.05 |
| gallery/compositors_proof.ps | 205,819 | 182,425 | -11.4% | 295 | 0.0302 | 22.99 |
| gallery/ring_of_type.ps | 205,492 | 168,514 | -18.0% | 509 | 0.0404 | 8.44 |
| examples/groovy_faces.ps | 204,359 | 175,203 | -14.3% | 101 | 0.0172 | 9.28 |
| examples/apparel_shirt.ps | 181,030 | 158,736 | -12.3% | 60 | 0.0148 | 4.42 |
| examples/paintkit_oil_demo.ps | 169,957 | 146,560 | -13.8% | 40 | 0.0045 | 3.45 |
| examples/font_library2.ps | 169,213 | 149,611 | -11.6% | 97 | 0.0166 | 3.14 |
| gallery/frost_mandala.ps | 161,803 | 133,503 | -17.5% | 829 | 0.0337 | 4.08 |
| examples/paintkit_liner_demo.ps | 158,809 | 133,170 | -16.1% | 85 | 0.0385 | 7.00 |
| examples/paragraph_layout.ps | 146,804 | 126,731 | -13.7% | 25 | 0.0158 | 9.20 |
| examples/scatter.ps | 134,224 | 117,153 | -12.7% | 56 | 0.0261 | 4.60 |
| examples/font_library.ps | 129,564 | 117,258 | -9.5% | 63 | 0.0088 | 5.04 |
| gallery/hortus.ps | 120,888 | 102,766 | -15.0% | 295 | 0.0136 | 19.00 |
| examples/keepin_it_real_shirt_chewy.ps | 119,880 | 113,354 | -5.4% | 13 | 0.0034 | 4.48 |
| examples/keepin_it_real_shirt.ps | 105,515 | 100,305 | -4.9% | 12 | 0.0039 | 4.23 |
| examples/keepin_it_real_front_alt.ps | 102,381 | 90,117 | -12.0% | 37 | 0.0106 | 2.90 |
| examples/apparel_second.ps | 101,578 | 88,641 | -12.7% | 14 | 0.0042 | 2.24 |
| examples/printkit.ps | 97,536 | 85,718 | -12.1% | 351 | 0.1972 | 19.36 |
| examples/jersey.ps | 96,957 | 81,680 | -15.8% | 623 | 0.0622 | 23.00 |
| gallery/lodestone.ps | 89,868 | 77,397 | -13.9% | 440 | 0.0231 | 19.00 |
| examples/pdf_document.ps | 82,536 | 69,587 | -15.7% | 12 | 0.0033 | 2.30 |
| examples/hyperbolic.ps | 78,247 | 69,547 | -11.1% | 70 | 0.0122 | 2.30 |
| examples/dataviz_paper.ps | 75,862 | 68,158 | -10.2% | 21 | 0.0071 | 4.34 |
| examples/style_toon.ps | 69,288 | 62,711 | -9.5% | 23 | 0.0063 | 6.18 |
| examples/noise.ps | 64,242 | 59,216 | -7.8% | 43 | 0.0246 | 13.02 |
| examples/dataviz.ps | 60,696 | 53,098 | -12.5% | 24 | 0.0074 | 6.90 |
| gallery/infinite_descent.ps | 58,883 | 51,777 | -12.1% | 231 | 0.0063 | 4.78 |
| gallery/silk_waves.ps | 58,140 | 50,851 | -12.5% | 823 | 0.0787 | 22.70 |
| examples/international_text.ps | 57,218 | 48,408 | -15.4% | 9 | 0.0041 | 5.00 |
| examples/specimen.ps | 57,125 | 48,550 | -15.0% | 34 | 0.0138 | 7.65 |
| examples/style_scifi.ps | 53,790 | 50,644 | -5.8% | 7 | 0.0022 | 2.24 |
| examples/type3_demo.ps | 53,385 | 44,984 | -15.7% | 52 | 0.0272 | 17.85 |
| examples/template_poster.ps | 52,835 | 44,485 | -15.8% | 39 | 0.0140 | 23.20 |
| examples/style_psychedelic.ps | 52,236 | 44,954 | -13.9% | 63 | 0.0115 | 4.60 |
| examples/hatching.ps | 52,171 | 46,227 | -11.4% | 88 | 0.0431 | 4.60 |
| examples/template_letter.ps | 50,529 | 42,881 | -15.1% | 21 | 0.0111 | 12.75 |
| examples/template_invitation.ps | 50,155 | 41,954 | -16.4% | 24 | 0.0067 | 6.12 |
| examples/paintkit_demo.ps | 50,131 | 42,548 | -15.1% | 22 | 0.0066 | 8.05 |
| examples/template_certificate.ps | 49,764 | 41,969 | -15.7% | 23 | 0.0092 | 10.05 |
| examples/graph_paper.ps | 48,706 | 43,756 | -10.2% | 11 | 0.0043 | 4.48 |
| examples/gradients.ps | 43,642 | 37,632 | -13.8% | 8 | 0.0042 | 5.10 |
| examples/headline.ps | 41,501 | 34,199 | -17.6% | 62 | 0.0044 | 4.44 |
| gallery/ripple_range.ps | 41,180 | 34,302 | -16.7% | 378 | 0.0239 | 4.30 |
| examples/graphing.ps | 38,990 | 36,238 | -7.1% | 45 | 0.0079 | 6.90 |
| examples/type3_ransom.ps | 38,065 | 31,746 | -16.6% | 46 | 0.0138 | 20.43 |
| examples/paintkit_nib_demo.ps | 36,077 | 30,417 | -15.7% | 31 | 0.0094 | 3.82 |
| examples/fractals.ps | 33,136 | 31,122 | -6.1% | 22 | 0.0092 | 2.42 |
| gallery/recursive_peaks.ps | 32,146 | 28,839 | -10.3% | 692 | 0.0346 | 12.20 |
| gallery/plum_branch.ps | 29,539 | 25,589 | -13.4% | 73 | 0.0041 | 4.18 |
| examples/etching_demo.ps | 27,692 | 24,494 | -11.5% | 78 | 0.1099 | 2.30 |
| examples/circular_text.ps | 24,915 | 20,690 | -17.0% | 12 | 0.0106 | 7.05 |
| gallery/varsity_backs.ps | 22,520 | 20,941 | -7.0% | 291 | 0.0090 | 21.40 |
| examples/tiling.ps | 21,783 | 20,063 | -7.9% | 51 | 0.0064 | 2.30 |
| examples/postcard.ps | 20,814 | 17,809 | -14.4% | 23 | 0.0122 | 17.85 |
| examples/template_card.ps | 20,478 | 17,429 | -14.9% | 10 | 0.0030 | 2.25 |
| gallery/woven_labyrinth.ps | 20,082 | 19,095 | -4.9% | 6 | 0.0007 | 2.00 |
| examples/handwriting.ps | 17,647 | 14,853 | -15.8% | 30 | 0.0076 | 4.44 |
| gallery/hundred_lines.ps | 17,066 | 14,589 | -14.5% | 151 | 0.0183 | 6.12 |
| gallery/field_notes.ps | 13,964 | 11,816 | -15.4% | 46 | 0.0024 | 2.19 |
| examples/walkpath_demo.ps | 12,086 | 10,392 | -14.0% | 3 | 0.0040 | 4.56 |
| gallery/nightfall_triptych.ps | 8,451 | 7,447 | -11.9% | 569 | 0.0402 | 1.84 |
| gallery/cathedral_rose.ps | 7,308 | 6,899 | -5.6% | 1,680 | 0.0608 | 5.85 |
| examples/hangul_handscript.ps | 4,580 | 4,136 | -9.7% | 8 | 0.0034 | 4.44 |
| examples/koch_snowflake.ps | 3,504 | 3,273 | -6.6% | 7 | 0.0057 | 2.55 |
| examples/sierpinski.ps | 2,674 | 2,664 | -0.4% | 0 | 0.0000 | 0.00 |
| examples/testcard.ps | 2,250 | 2,215 | -1.6% | 3 | 0.0017 | 1.97 |
| examples/stage2_demo.ps | 1,569 | 1,529 | -2.5% | 6 | 0.0017 | 1.86 |
| examples/golden_spiral.ps | 1,423 | 1,370 | -3.7% | 3 | 0.0005 | 2.30 |
| examples/sweep_demo.ps | 1,274 | 1,250 | -1.9% | 2 | 0.0022 | 0.84 |
