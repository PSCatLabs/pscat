# #215: Released in Part, the typed-memo gallery piece, and the blind comparison

Epic #209's last ticket. Builds on #211–#214; read
`notes/214-released-look.md` first. The design is `docs/dev/design/TYPEWRITER.md`.

## What shipped

- `gallery/released_memo.ps` (*Released in Part*), letter size, and its 2x still
  `gallery/renders/released_memo.png`. A one-page memorandum of November 1962
  from an invented "Upper Basin Survey Office" about two lost cans of aerial
  film (Project GREYWING, Harlan Flats: all made up). It has:
  - the `/Memo` header (To, From, Cc, Subject, Date);
  - five numbered paragraphs with a tab after the number, and a signature
    block and typist's initials placed by tabs;
  - a strikeover (`<o>was|were</o>`), an x-out (`<x>sixty</x>`), two underlined
    phrases and two redactions (`<r>`, drawn by `rel-redact`);
  - ribbon wear: elite (`/Pitch 12 /Size 11.5`), `/Wear 1.3`, `/Ribbon 0.55`,
    a slightly brown-black `/Ink`;
  - `rel-page`: aged paper, folds, two punch holes, scan skew and lamp, one copy
    generation, a RECEIVED date stamp, a RELEASED IN PART stamp, handwritten
    initials over the signature and a margin note.
- `gallery/manifest.json` entry (lettering), and `typewriter`,
  `typewriter_outlines` and `released` in its library registry (guide:
  `docs/guides/fonts.md`). The READMEs' tables are regenerated.
- `docs/dev/design/typewriter/tools/blind_compare.py` writes:
  - `docs/dev/design/typewriter/blind_grid.png`: six strips, A to F, in a fixed shuffled
    order: four from real scans (T2, T9, T10 and T15; three documents) and two
    from the render. Every strip is one typed line at one scale (240 dpi, the
    Szilard scan's own) and one size (720 x 64 px), greyscale; the render's go
    through JPEG at quality 85. So no tile can be told apart by size or type
    scale. The real crops are resampled from their committed scale: T2 by 0.8,
    T9 by 1.67 (the one real enlargement), T10 by 0.67, T15 by 1.14. T4, at
    600 dpi, is too small for a strip at 240. The key is printed, not
    committed: it is in the PR, so the maintainer can judge first.
  - `docs/dev/design/typewriter/memo_vs_real.png`: the page beside T1 at T1's scale
    (75 dpi).
    T1 (a 1914 letter) is the board's only full typed page; it is not a
    released memo, but it is the closest real page in the repo.
- Docs: a paragraph in `docs/guides/fonts.md` (the released-document section),
  `HANDOFF.md`. The issue asks for NOTES.md; this repo's convention since is
  `notes/<N>-*.md`, so this file is that record.

## Decisions

- **One copy, with the type kept open.** `rel-page` passes `/Generation` to the
  type as `/Copies`; at 1 that already filled most counters and looked heavy
  at 300 dpi. The piece sets `/Copies 0.35` in the
  `tw-type` dict (the library keeps the content's own value), so the page goes
  through the copier but the type closes up only a little. Recorded as a tip in
  the guide.
- **Elite, not pica.** Three of the four reference crops are elite; a pica crop
  would stand out by pitch alone.
- **One scale, one size.** A first board gave each render crop the pixel size
  and scale of one scan (T4's, T9's); that paired each render tile with exactly
  one real tile and gave the answer away. Every strip now shares a size and a
  scale.
- **Crops of body text only.** The header carries invented names; the strips
  also keep away from the redaction bars, the punch holes and the handwriting,
  since no real strip has any of them.
- **The judge is the maintainer.** The agent who built the piece has seen every
  reference, so its reading of the grid is not blind; the PR reports it as an
  assessment only.
- **gs.** The piece renders in Ghostscript with the command in its header
  (`-dNOSAFER`, `-g`); the page matches pscat's except for the paper grain (as
  #214 noted) and gs's thinner antialiased strokes at low resolution.

## The comparison

The maintainer judges the grid (the issue's success criterion). The author's
own reading, which is not blind (the author has seen every reference), is in
the PR under the key, so that nothing in the repo gives the answer away first.

## Not done (filed, low priority)

- #466: a copy's paper keeps no grey haze or grain.
- #467: the 4x mis-strike on y lifts a letter beyond the measured tail.
- #468: the reference search docs/dev/design/TYPEWRITER.md Q3 asked of this ticket
  (two-colour bleed, embossing, a confirmed electric page) was not done.
- Already open: #448 (multi-page `rel-page`, so the piece is one page), #449,
  #450, #451, #422, #423.
