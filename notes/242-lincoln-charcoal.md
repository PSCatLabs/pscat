## Gallery: an art student's charcoal portrait of Lincoln (issue #242, 2026-10-14)

`gallery/lincoln_charcoal.ps`, *Study after Berger, 1864*: a charcoal
portrait study of Abraham Lincoln on drykit's drawing paper, drawn with
drycharcoal (#241) to the tones of a public-domain photograph.

**The photograph.** Anthony Berger, manager of Mathew Brady's Washington
gallery, 9 February 1864 (Ostendorf O-91). Source: the Library of Congress
print, via Wikimedia Commons,
[File:Abraham Lincoln O-91 by Berger, 1864.png](https://commons.wikimedia.org/wiki/File:Abraham_Lincoln_O-91_by_Berger,_1864.png),
marked public domain by both. It is a US work published before 1929. The
Commons/LoC description names it the model for the five-dollar bill's
portrait. `gallery/references/lincoln_berger_1864.jpg` is a 480 × 600
greyscale crop of the head and shoulders (source pixels 140..780 by
60..860, alpha flattened onto white, baseline JPEG, no EXIF, 46 KB). The
page is the same 480 × 600, one point per photo pixel, so feature
positions were measured off the crop on a gridded zoom (eyes, brows, nose,
mouth, ear, collar, tie).

**`et-tonemap` (lib/etching.ps).** The issue asked for the tonal map to
reuse etching.ps's photo sampling. et-draw's decoding was private, so it
moved into an internal `et-loadbuf`, shared by et-draw and a new public
`et-tonemap`:
- `<< /Photo /Columns /Rows /Samples >> et-tonemap` returns rows of cell
  darkness in [0,1]. Row 0 is the top of the photo, and EXIF orientation is
  honoured through the same `et-orient-map`.
- Each cell averages an n × n set of samples, so the map is box-filtered
  rather than aliased.
- The piece uses 240 × 300 cells at 2 × 2 samples. Building it takes about
  0.5 s in pscat.

Four tests in `tests/etching.rs` cover it:
- shape, range and image order;
- a flat photo maps flat, and the operand stack stays clean;
- EXIF orientation 6;
- the option guards.

et-draw's behaviour is unchanged: the existing etching tests pass.

**How the piece is drawn**, in a student's order:
1. `dhsketch` construction lines: the head's oval, a tilted centre line,
   brow, eye, nose, mouth and chin lines, the shoulders and a plumb line.
2. `dhsmudge` stump strokes on a jittered grid. They go only where the
   photo is darker than its backdrop (darkness over 0.62) and the drawing
   is well finished. Their tone stays below the hatching that covers them.
3. Three `dhhatch` passes, each with a `{x y -> value}` procedure that
   reads the map bilinearly:
   - the head in pencil: `/Length 10`, `/Spacing 2.2`;
   - the features in finer pencil: `/Length 6`, `/Spacing 1.5`, cross-faded
     with the head pass by an ellipse;
   - the coat in vine: `/Length 26`.

   A fourth, vine pass at -35° crosses only the darkest masses. Each value
   is darkness stretched (0.3..0.78 to 0..1) times a finish mask. The mask
   is an ellipse round the head, plus a wider one round the shoulders that
   fades out toward the page's foot. A slow sine wobble makes the faded
   edge ragged.
4. Accents:
   - brows as ten short compressed strokes each, at hashed offsets;
   - irises as small compressed rings;
   - lids, nose, mouth, folds, ear, jaw, collar and bow tie in pencil;
   - the lapels as two light vine strokes.
5. Hard-eraser `dhlift` streaks in the hair and on the right lapel. Each
   one's `/Under` is the tone the hatching left there.

No rand draws: jitter comes from a sin-based hash of grid indices.
pscat and gs renders match by eye. The mean luma difference at 144 dpi is
14.7 of 255. That figure was not checked against a control. gs's 32-bit
reals (the hash multiplies `sin` by 43758) and its DCT rounding can each
move a mark or shift a hatch threshold.

`et-tonemap` checks its options before `EtchLayout begin`, so a refused
call leaves the dict stack clean, and a test pins this.
`et-loadbuf`'s own errors still leave `EtchLayout` on the dict stack, as
they always have in `et-draw`. That covers a file that isn't a JPEG and
an unsupported component count. `et-loadbuf` now closes `et-scan`'s file
handle, which et-draw used to leave open.


**The piece's names** carry an `lc` prefix (`lcP` and so on), because
`/P` is a top-level name in `lib/strokeglyphs.ps`.

**Review rounds (art).** Nine renders, in two passes.

The first pass fixed bugs and structure:
- pupils drawn at unflipped y;
- region polygons with a visible straight seam;
- lifts over light skin that drew grey bars, because a kneaded lift's
  halo is its `/Under` colour.

The second pass was about the look:
- Smudge sausages and clouds at every mass edge. Smudges were limited to
  inside dark masses, made smaller and denser, and kept at 0.42 × the
  hatch value.
- Eyes reading as spectacles. Fixed with softer lids, broken-stroke brows
  and iris rings.
- The features smearing. Fixed with the finer feature pass and a 2 px map.

**Measured** (CPU `user`, 144 dpi, machine load about 4):

| Render | pscat | gs 10.07.1 | Budget (DRYMEDIA.md 6.2) |
|---|---|---|---|
| `--dpi 144` / `-r144` | 23.3 s | 21.6 s | 60 s |
| the 2× gallery still (`2 2 scale`, `--page 960x1200`) | 24.9–29.8 s | | |

The 2× still is pixel-identical to the `--dpi 144` render.

**Comparison:** `docs/drymedia/lincoln_compare.png` (built by
`docs/drymedia/prototypes/lincoln_compare.py`). It shows the photo and the
render, over the 1888 third-year portrait (H3), the 1878 cast study and the
H2 hatching crop.

**Gaps against the real student studies:**
- **The tone is built mostly from hatching.** H3 and the 1878 cast study
  are mostly stumped tone with hatching on top. The render's hatching is
  crisper and more regular, especially the cross-hatched coat. A
  `dhsmudge` shows its own outline wherever the hatching over it is light,
  so the stump could only go where hatching hides it. This is #241's
  "smudged tone is flatter than H1" gap, seen from the other side.
  drykit can't read the page back, so a smudge can't soften what is
  already there (#134).
- **Lifts.** A kneaded lift over anything lighter than its `/Under` draws a
  grey band with a pale rim, so the piece uses the hard eraser's crisp
  streaks instead. Real kneaded lifts (H8) are soft.
- **Newsprint.** The piece is on drawing paper. Newsprint's warm grey
  ground would make the lifts read better, but it compresses the value
  range the likeness depends on. Charcoal on newsprint is still not
  sourced on the #233 board.
- **Construction lines** were drawn from the usual head-proportion
  conventions. There is still no committed reference crop of them.
