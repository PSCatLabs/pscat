# Typewriter reference board (issue #210)

This is the reference board for epic #209 (`lib/typewriter.ps`). Each entry
is a real scan, what it shows, and what it means for the library.
`docs/dev/design/TYPEWRITER.md` turns these observations into a design. #211, #212 and #215
compare their renders against the crops here.

![Reference board](reference_board.jpg)

Rules followed:

- Every observation was made by looking at the scan, and most were also
  measured on it (see Measurements). A trait from #210's list that has no
  image behind it is marked **not yet sourced**.
- Every committed crop comes from a public-domain source: a work of the US
  government (17 U.S.C. § 105; Commons tag `{{PD-USGov}}`), or, for the
  Szilard petition, a work published without a copyright notice (Commons tag
  `{{PD-US-no notice}}`). The Status column gives the tag read from each file
  page. The O'Brien memo is measured but **not** cropped: see its row.
- Real agency letterheads, seals and classification markings appear in some
  crops (T1, T11, T14, T15). They are here as references only. The epic does not
  allow them in specimens: specimens use fictional organizations.
- `tools/make_board.py` rebuilds `refs/` and `reference_board.jpg` from the
  sources. `tools/measure.py`, `tools/mono.py` and `tools/stem.py` produce the
  numbers below. They need Pillow, NumPy and tesseract.

## Sources

| Key | Document | Typewriter | Scan | Source | Status |
|---|---|---|---|---|---|
| Duncan | J. J. Duncan to Supt. W. C. Kohlenberg, 17 Aug 1914 | manual, pica (10 cpi), double spaced | 2411×3150 TIFF, 300 dpi | [Commons, NARA 284501](https://commons.wikimedia.org/wiki/File:J.J._Duncan_to_Superintendent_W.C._Kohlenberg,_Crow_Creek_Agency,_South_Dakota._-_NARA_-_284501.tif) | `{{PD-USGov}}` (an Indian Service superintendent's letter) |
| FDR | "Day of Infamy" first typed draft, 7 Dec 1941, pp. 1–2 | manual, elite (12 cpi), triple spaced | 2400×3152 JPEG, 300 dpi | [Commons, via DPLA / FDR Library](https://commons.wikimedia.org/wiki/File:Annotated_Draft_of_Proposed_Message_to_Congress_Requesting_Declaration_of_War_Against_Japan_-_DPLA_-_2eb3ebba93444052d0046022975cbff5_(page_1).jpg) | `{{PD-USGov}}` (the President's own draft) |
| Szilard | Petition from Leo Szilard and other scientists to President Truman, 17 Jul 1945 | manual, elite, single spaced | 2069×3150 colour JPEG, about 240 dpi (inferred, see below) | [Commons, NARA 6250638](https://commons.wikimedia.org/wiki/File:Petition_from_Leo_Szilard_and_Other_Scientists_to_President_Harry_S._Truman_01.jpg) | `{{PD-US-no notice}}` (written by Manhattan Project scientists, not a federal work) |
| O'Brien | Larry O'Brien, memorandum to Kennedy coordinators, 1960 | monospace elite; electric inferred (see below) | 2551×3300 JPEG, 300 dpi | [Commons, NARA 194005](https://commons.wikimedia.org/wiki/File:Larry_O%27Brien_Memorandum_to_Kennedy_Co-ordinators_and_Campaign_Directors_-_NARA_-_194005.jpg) | Commons tags it `{{PD-USGov}}`, but a 1960 campaign memo is not plainly a federal work. **Measured and linked, not cropped.** |
| MFR 1955 | Eisenhower, Memorandum for the Record, 31 Jan 1955 | proportional (IBM Executive type) | 1686×2200 JPEG, 200 dpi | [Commons, NARA 186540](https://commons.wikimedia.org/wiki/File:Memorandum_for_the_Record_-_NARA_-_186540.jpg) | `{{PD-USGov}}` |
| NSAM 55 | National Security Action Memorandum No. 55, 28 Jun 1961 | proportional, bold | 2428×3150 JPEG, 300 dpi | [Commons, NARA 193454](https://commons.wikimedia.org/wiki/File:National_Security_Action_Memorandum_No._55_Relations_of_the_JCS_to_the_President_in_Cold_War_Operations_-_NARA_-_193454.jpg) | `{{PD-USGov}}` |
| JFK 10106 | CIA memorandum to the FBI, May 1961, JFK Assassination Records 104-10297-10106 | monospace; bitonal release copy | PDF, rasterized at 300 dpi | [archives.gov](https://www.archives.gov/files/research/jfk/releases/104-10297-10106.pdf) | US federal record (a CIA memorandum), 17 U.S.C. § 105 |
| NBS C431 | C. E. Waters, *Typewriter Ribbons and Carbon Paper*, National Bureau of Standards Circular C431 (about 1941) | text only | — | [Internet Archive](https://archive.org/details/circularofbureau431wate) | public domain (US government publication) |

These sources were viewed but no crop was committed:

- *On the Makaloa Mat* typescript (Commons). A museum photograph at 72 dpi, and
  its licence was not checked.
- The Eisenhower memorandum by A. J. Goodpaster (NARA 186630), and C. E. Hawley's
  CIA memorandum of 1954 (Commons). Both are proportional faces, as the MFR 1955
  scan is, and add nothing to it.
- 48 JFK release PDFs (index pages 226, 227, 300 and 400 of the 2017–2018
  release), OCR'd page by page and searched for `xxxx` overtypes. None was
  found.

### How pitch and resolution were checked

The scans' resolution tags are not trusted on their own. Each one was
cross-checked from the text:

- Character pitch was found by autocorrelating ink columns along a line. A grid
  fit (`tools/mono.py`) gives the median distance of each character from a
  fixed-pitch grid, in fractions of the pitch: about 0.25 for random
  placement, under 0.08 for monospace.
- Line pitch was found by autocorrelating ink rows. Typewriters advance by 1/6
  inch per line, so the ratio of line pitch to character pitch is 1.67 for
  pica (10/6) and 2.0 for elite (12/6). This ratio does not depend on the scan's
  dpi.

| Key | Character pitch | Line pitch | Ratio, and verdict | Grid fit |
|---|---|---|---|---|
| Duncan | 29.75 px | 98 px (double) | 49/29.75 = 1.65, so **pica**. 300 dpi gives a 24.0pt double space (measured 23.7pt). | 0.047 |
| FDR | 25.1 px | 151 px (triple) | 50.3/25.1 = 2.0, so **elite**. 300 dpi gives a 36pt triple space (measured 36.0pt). | not measured (handwriting) |
| Szilard | 20.0 px | 40 px (single) | 2.0, so **elite**. That makes the scan 240 dpi, an inferred figure. | 0.034 |
| O'Brien | 24.5 px | 49.6 px | 2.0, so **elite**, at 300 dpi. | 0.063 |
| JFK 10106 | 20.0 px | — | Not determined: the PDF's page size need not match the paper's, so 20 px at 300 dpi rasterization does not fix the pitch. | 0.097 (monospace, degraded) |
| MFR 1955 | — | — | **proportional** | 0.187 |
| NSAM 55 | — | — | **proportional** | 0.168 |

## Observations

| # | Crop | What it shows | What it means for the library |
|---|---|---|---|
| T1 | `T01_duncan_page_1914.jpg` | A whole 1914 letter. Pica, double spaced, with a printed letterhead and a faint embossed seal. The body is set in from a fixed left margin, with paragraph indents of 5 and 10 columns. | Page layout for #213: a fixed left margin, indents in whole columns, the date and signature placed by tab stops. |
| T2 | `T02_duncan_line.jpg` | "In reply to your letter of June". The first `e` of "letter" prints light and thin. The `r`s print heavy. Stems are about 1.44pt wide, against about 0.9pt for a clean Courier at 12pt: the ink spreads well past the type face. | Strike weight varies per strike and per key. Ink spread, not the base face, sets the stroke weight. |
| T3 | `T03_fdr_strikeover.jpg` | In "Yesterday", the `s` is typed over another letter in the same cell. Both remain legible, darker where they overlap. | A strikeover is a second strike at the same cell. It must get its own jitter, so the two shapes don't coincide. |
| T4 | `T04_fdr_filled_counters.jpg` | "a date which". The ink is mottled at a fine scale of about 0.5pt (2 px at 300 dpi), not flat. The bowl of `a` is filled solid. The bowl of `e` is partly filled. | Ink texture at the ribbon's yarn scale (NBS C431: about 150 yarns per inch, or 0.48pt). Counters fill. |
| T5 | `T05_fdr2_strikeover.jpg` | "Government", with an `m` struck over an `n`. | A second example of M8: a correction by overtyping. |
| T6 | `T06_fdr_a_tiles.png` | Every `a` on FDR page 1, tiled. Nearly all show the same filled bowl. | A filled counter is a property of the machine (dirt in that slug), not chance. It belongs in the per-machine layer, with per-strike variation in how full. |
| T7 | `T07_duncan_e_tiles.png` | Every `e` in the Duncan letter. The shape is constant. Weight varies, and one strike is almost missing. | Per-strike pressure. A rare very light strike is real. |
| T8 | `T08_szilard_e_tiles.png` | Every `e` in the Szilard petition. The shape and the grey tone are very even. | A good machine with a worn ribbon: low variation, low contrast. |
| T9 | `T09_szilard_block.jpg` | Elite, single spaced, justified only on the left, with a ragged right edge. Paragraphs indent 10 columns. Hyphenation at line ends ("im-/posed"). The paper is toned. | Layout for #213. The look is set by margin regularity: the left edge is very straight (0.17pt sd). |
| T10 | `T10_szilard_zoom.jpg` | The ink is grey-brown, not black (mean darkness 0.63, against 0.76–0.81 elsewhere). Edges are soft but not ragged. | Ink colour and strength are parameters. A worn ribbon is lighter, not just patchier. |
| T11 | `T11_nsam55_underscores.jpg` | A heading underlined by typed underscores, as one continuous rule, and the item letters `a.` underscored. The O'Brien memo (not cropped, linked above) shows the same on monospace elite: a continuous underscore under each word, with very even weight. | The underscore is a separate strike in the same cell, after backspacing. Its rule joins across cells. |
| T12 | O'Brien memo, item C (not cropped; [file page](https://commons.wikimedia.org/wiki/File:Larry_O%27Brien_Memorandum_to_Kennedy_Co-ordinators_and_Campaign_Directors_-_NARA_-_194005.jpg)) | One paragraph of the memo: many letters print only their top or bottom half ("use one", "party"), with stray marks. | A strike can land unevenly, printing one side heavier, or one side not at all. This is the "prints heavier on one side" trait. Here it may come from a stencil or duplication rather than the typewriter. |
| T13 | `T13_mfr1955_erasure.jpg` | A proportional executive face. In the margin, a ghost of erased typing ("agr") remains as a faint grey. Weight varies strongly per letter. | An erasure leaves a faint ghost of the original strikes: a correction for #213 or #214. Proportional faces are out of scope (see TYPEWRITER.md). |
| T14 | `T14_nsam55_marks.jpg` | A black redaction bar, a "DISPATCHED" date stamp, a declassification stamp with handwriting, and a struck-through typed line. | Released-document marks for #214. Specimens must invent their own: no real markings. |
| T15 | `T15_jfk_release_copy.jpg` | A bitonal release copy. Counters are filled, thin strokes have dropped out, and specks are scattered over the page. | The "released copy" degradation for #214. It sits on top of the typewriter look, so #211 and #212 tune against T2–T10, not this. |

### Text source: NBS Circular C431 (about 1941)

- "Typewriter ribbons, with few exceptions, are made from a thin, closely
  woven cotton fabric. The weave is plain." Federal specifications required at
  least 135 yarns per linear inch, later 300 per square inch (about 150 each
  way). So the yarn pitch is about 0.17 mm, or 0.48pt.
- "If the yarns are thick and the weave is open, the strokes of the individual
  characters will be made up of dots, instead of being solid lines." Ribbon
  texture shows as dots along the strokes, and more so on a dry or coarse
  ribbon.
- "Any typewritten character is a little less sharp than the face of the type,
  because slight roughnesses of the ribbon and the paper make the two touch one
  another in spots slightly beyond the sharp edges of the type." This is the
  ink spread and the feathered edge.
- Pica is 10 characters per inch, elite 12. A 7-inch line holds 70 or 84
  characters, and there are 6 lines to the inch.
- Carbon copies are larger and blurrier, and each further sheet in the pack
  more so. Elite gives fewer legible carbons than pica.

## Measurements

These come from `tools/measure.py`. Tesseract gives each character's identity
and box. Each line gets a robust fit: a grid at the measured pitch for x, and
a straight baseline for the characters with no descender. Residuals from those
fits are in points.

**Resolution limit:** one pixel is 0.24pt at 300 dpi and 0.30pt at 240 dpi,
and OCR boxes are whole pixels. Standard deviations of 0.15–0.3pt are at the
one-pixel level, so read them as upper bounds. The ranges are what matter: the
real noise is a fraction of a point, not points.

| Quantity | Duncan 1914, pica | Szilard 1945, elite | O'Brien 1960, elite |
|---|---|---|---|
| characters measured | 368 | 2707 | 1875 |
| per-strike horizontal sd (all characters) | 0.31 | 0.25 | 0.31 |
| per-strike horizontal sd within one letter | 0.25 | 0.22 | 0.27 |
| per-strike vertical sd (baseline residual) | 0.29 | 0.19 | 0.16 |
| per-strike vertical sd within one letter | 0.20 | 0.18 | 0.14 |
| per-letter vertical offset sd, raw | 0.17 | 0.10 | 0.08 |
| per-letter vertical offset sd, face shape removed* | 0.09 | 0.06 | 0.08 |
| largest per-letter vertical offsets, face shape removed | `w` −0.17, `s` −0.16, `c` +0.16 | `S` +0.14, `d` +0.10 | `w` +0.23, `c` −0.13 |
| strikes off by more than 0.5pt vertically | 6.6% | 0.8% | 0.1% |
| strikes off by more than 1pt horizontally | 0.8% | 0.9% | 0.2% |
| baseline tilt per line, sd about the page's skew | 0.09° (max 0.20°) | 0.06° (max 0.16°) | 0.10° (max 0.19°) |
| line-start wander, sd with page skew removed | 0.24 (one group) | 0.17 | 0.38† |
| line advance: median, and sd | 23.66, 0.43 (double) | 12.21, 0.16 | 11.90, 0.33 |
| inked stem width | 1.44 | 0.90 | 1.44 |
| ink coverage of the glyph box: per-strike coefficient of variation within a letter | 0.12 | 0.09 | 0.07 |
| ink darkness of glyph pixels: mean, and sd of the per-letter means | 0.76, 0.023 | 0.63, 0.005 | 0.81, 0.006 |

\* A letter's mean vertical offset mixes the machine's quirk with the face's
design (round letters overshoot the baseline, for example). Subtracting the mean
over the three documents for each of the 19 letters they share removes most of
the design part. With three documents, that subtraction also removes about a
fifth of the true spread. So the per-machine figure is about 0.07–0.11pt.

† O'Brien's T12 paragraph may be a pasted-in correction, which would inflate its
line-start figure.

FDR's pages were left out of the line and placement figures, because
handwriting on the page confuses the fits. Its ink figures are kept: coverage
CV 0.21, stem 0.96pt. They show a heavier, more uneven ribbon than the others.

**Rotation per strike could not be measured.** Fitting the centre line of
`l`/`I`/`1` stems gives a spread of 1.6–5°. That is the noise of fitting 10–20
pixels of stem, not real rotation: at full size no strike on the board looks
rotated by more than about 1°. The design uses a small per-strike rotation and
leaves larger tilts to a few per-machine "bent slugs".

**Electric, by inference.** O'Brien 1960 is the only monospace sample that
might be electric. The evidence is circumstantial: per-letter darkness is about
four times more even than Duncan's (0.006 against 0.023), it has the lowest
coverage CV (0.07), and its underlining is continuous. A manual typist's uneven
finger force produces exactly the variation that is missing here. Szilard's
1945 petition is nearly as even (0.005), so evenness alone doesn't prove an
electric machine. The design therefore treats "electric" as a preset with low
pressure variation, not as a separate model.

## #210 traits

| Trait in #210 | Status |
|---|---|
| manual and electric typewriters | Manual: T1–T10. Electric: **inferred only**, from the O'Brien measurements (see Electric, by inference). |
| pica and elite pitch | Pica: T1, T2. Elite: T3–T10. Both are verified from the line/character pitch ratio. |
| per-machine quirks: high or low letters, lean, one side heavier | High/low letters are measured (0.07–0.11pt sd, at most about 0.23pt). The clogged `a` (T6). One side heavier: T12 (observed, not cropped). **Lean is not resolved** at these scans (see Rotation). |
| per-strike offset, rotation, pressure | Offset is measured (0.2–0.3pt sd). Pressure: T2, T7 and coverage CV 0.07–0.21. Rotation: upper bound only. |
| light strokes from weak fingers | T2, T7 |
| punctuation that embosses the paper | **Not yet sourced.** Periods print dark and round (T1, T2), but no scan shows raking light, so embossing is not visible. |
| ribbon weave showing through | T4 at about 0.5pt, and NBS C431's description. |
| feathered edges | T2, T4, and C431's "touch ... slightly beyond the sharp edges". |
| filled counters | T4, T6, T15 |
| wearing or drying ribbon | T10 (worn, grey-brown) against T2 (fresh, black). |
| two-colour ribbon bleed | **Not yet sourced.** One bounded search on Commons and the web found no red/black typescript scan with a licence that could be checked. |
| baseline drift along a line | Measured: 0.06–0.10° sd per line. |
| line-start wander after each carriage return | Measured: 0.17–0.38pt sd. |
| uneven platen advance | Measured: 0.16–0.43pt sd. |
| strikeovers | T3, T5 |
| `xxxx` over words | **Not yet sourced.** Nothing turned up in a search of 48 JFK release PDFs by OCR. It is a strikeover repeated along a word (T3, T5), so the design covers it. |
| underscores made by backspacing | T11 |
