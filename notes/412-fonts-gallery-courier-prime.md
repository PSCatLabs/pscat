## Courier Prime and /Typewriter in the font gallery (issue #412, 2026-10-14)

`scripts/build_font_gallery.sh` lists faces explicitly, and #211/#212 added
a face and a Type 3 font without adding a card. Now there are two:

- **`catalog/courierprime.png`**: a `CourierPrime|CourierPrime-Regular|...`
  row next to SpecialElite's, same label/sample/size pattern as the other
  catalog cards (sample `Hamburgefonstiv 12`, 22 pt; the trailing `12` is
  there to show the figures of a typewriter face). Site card in
  `site/fonts.html` right after SpecialElite's.
- **`type3/typewriter.png`**: yes, `/Typewriter` gets a card. It is dynamic
  like `/HandScript`, whose card is also a single render of a program, so
  the card has to show the variation, not a word. It is one key (`e`)
  struck twenty times at `/Size 30 /Pitch 4` (the pitch that matches the
  size: advance = 600 units, so pitch is about 72 / (0.6 x size)), then a
  caption line at pica pitch. `/Machine 7 /Seed 3 /Wear 1.5`: machine 7's
  clogged `e` (counters filled on some strikes), the line tilt, and the
  per-strike offsets and pressure are all visible. Wear 1.5 rather than the
  default 1 because at card resolution (1 px per point) the default
  variation is subtle by design.
  - The card needs both `lib/typewriter_outlines.ps` and `lib/typewriter.ps`.
  - It does not use `tw-type` (#213, in review): `tw-setup` and `show` only.
  - No `srand`: the library hashes `/Machine`, `/Seed` and the cell and
    never calls `rand`, so the card is deterministic as it is. The same
    program through Ghostscript (`gs -dNOSAFER`) draws the same strikes.
- Counts on `site/fonts.html` and `docs/guides/fonts.md` are fixed for the
  two cards: nine Type 3 faces, a 38-family display shelf (recounted from
  the page: 37 cards plus CourierPrime), 40 more display and text families
  in the intro (the shelf plus Chewy and BagelFatOne). The "62-file" figure
  and the guide's "61 outline faces" are not recounted here (the files on
  disk are more than that now); that is a separate cleanup.
- `docs/guides/fonts.md` is left almost alone (#432 rewrites its typewriter
  section): one count and one sentence on the gallery.

The gallery PNGs are not checked in (the script regenerates them into
`_site/assets/fonts`), so the PR carries no images; the maintainer's
before/after are the two new cards.
