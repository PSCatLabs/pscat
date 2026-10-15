## Released in Part, the final pass, and a new blind board (issue #531, 2026-10-15)

The capstone of the typewriter work: `gallery/released_memo.ps` (#215) re-rendered with
everything that landed since, and made the complement of *The Man from 2026*
(`gallery/time_traveler_memo.ps`, #590). That piece is a ribbon original that never
left the file. This one is the declassified copy: reviewed, redacted, stamped and
photocopied twice. Before and after, at 2x: `notes/531-compare.png`.

## What changed in the piece

- **Even hand.** It uses #590's settings, `/Wear 1.0 /MachineWear 0.4 /StrikeWear 0.5 /Ribbon 0.75`.
  The user rejected #590's first render and #594 round 1 for very dark letters next to
  very light ones. The old `/Wear 1.3 /MachineWear 0.6 /Ribbon 0.55` had the same swing.
- **Machine 26, not 11.** Machine 11 clogs `6@BRq`, which printed "■asin", "SU■JECT"
  and "■ecords". A probe of the machine table for machines 1 to 40 (the clog fraction
  above 0) shows 26 clogs only `8` and `q`. The memo types no 8. The `q` of "quarterly"
  and "inquiries" is filled but still reads. Machine 6 clogs nothing, but a clogged key
  is one of the traits worth showing. Machine 12 (`@e`) belongs to the sibling.
- **The release apparatus**, all of it invented:
  - an OFFICE USE ONLY marking typed at the head and foot with `tw-set` on the same
    frame (#590's `ttat` pattern, here `rmat`). It is ruled through by two passes of a
    reviewer's pen, drawn in the content proc so that the copier copies it. Paragraph 2
    already says the flight sheets were "marked for office use only".
  - an exemption code "R3" stamped beside each redaction bar, in the margin.
  - a BASIN RECORDS REVIEW box stamp (case, document number, BY / DATE), with
    handwritten initials and a date in the blanks.
  - a document number at the foot (`UBSO 000417`).
  - the RECEIVED and RELEASED IN PART stamps, kept.
- **Two copies** (`/Generation 2 /Haze 0.8`): a greyer haze with grain (#466, #474,
  #528), specks, the lid's shadow, and #450's soft fringe on every mark. The type keeps
  `/Copies 0.35`, which is `tCopyC` 0.19. #547/#594's erosion is active but light
  (`tEy` at most 45 x 0.19), and the counters stay open. Letting the type ride up to
  generation 2 would bring back the fading the user rejected.
- **A gem clip** (`/Clip 1`, #451), copied with the page.
- **The text** gains a paragraph about the vault log, which fills the page. The second
  enclosure is now named in paragraph 5, and the end reads "Enclosures (2)". The
  margin note moved under paragraph 6, and the initials sit beside the signature.
- **Not used:**
  - Multi-page (#448/#589). The gallery shows one still, rendered with `2 2 scale`
    prepended, which a second page does not keep (#590's reason too).
  - A red ribbon (#422). On a copy it would have to turn grey anyway, and today it
    does not: typed ink skips the copier's curve. Filed as #595.
  - `/Bleed`, `/BleedGap`, `/WornBand` and a raised `/MisStrike`. The default
    mis-strike rate stays. The others are looks that most real ribbons do not show
    (notes/468), and they read as noise at a copy's contrast.

## Evenness, measured

A probe takes a 300 dpi render of the body. It finds each word as a run of ink (pixels
under 170), drops the redaction bars, and averages each word's ink darkness over its
inked pixels, so the measure does not depend on how much ink a word's shape holds.
Across the words, the spread of that mean:

| page | words | mean | sd | cv |
|---|---|---|---|---|
| before (main, `/Wear 1.3`) | 126 | 0.711 | 0.036 | 0.051 |
| after | 178 | 0.746 | 0.029 | 0.039 |
| *The Man from 2026* (an original) | 437 | 0.718 | 0.016 | 0.022 |

A copy adds its own variation: the copy curve acts on each strike's level, and the
specks add to it. In a sweep at an earlier `/Haze 0.5`
(cv 0.042 there), `/Copies 0` gave 0.035 and `/StrikeWear 0.3` gave 0.035. By eye, at 2x, 0.3 was indistinguishable from 0.5, so the piece keeps #590's 0.5.

## The blind board

`docs/dev/design/typewriter/tools/blind_compare.py` uses #215's method, unchanged:
- four real strips (T2, T9, T10, T15) and two from the render;
- one scale (240 dpi) and one size (720 x 64);
- greyscale, with the render's strips passed through JPEG at quality 85.

The changes:
- new render origins for the new layout, body text only, away from the bars, codes,
  marking, stamps and handwriting;
- `SHUFFLE_SEED` 531, so #215's published key does not answer the new board.

The key was printed to stdout and is in a collapsed comment on the PR, not in the
repo. `memo_vs_real.png` was regenerated as well.

## Commands

    { printf '2 2 scale\n'; cat gallery/released_memo.ps; } > /tmp/hi.ps
    pscat --page 1224x1584 --png gallery/renders/released_memo.png /tmp/hi.ps
    python3 -I docs/dev/design/typewriter/tools/blind_compare.py
    gs -q -dNOSAFER -dBATCH -dNOPAUSE -sDEVICE=png16m -r72 -g612x792 \
       -sOutputFile=gs.png gallery/released_memo.ps </dev/null    # ok

## Filed

- #595: typed ribbon colour is not greyed on a copy. Stamps and notes are greyed; the
  ink of `tw-type` is not.
