## Gallery: the two-sided PSCat business card (issue #974, 2026-11-02)

Part of epic #966 (QR-7), on `feature/qr-code-library`. `gallery/pscat_card.ps`
("The Program Card") draws a 252 x 144 pt (3.5 x 2 in) card as two pages:
the front and the back. `lib/qr.ps` is unchanged.

**One source, every exporter.** `--pdf` writes one two-page PDF at true size,
the file to print from. `--png` and `--svg` write `card-001` (front) and
`card-002` (back), the CLI's existing multi-page naming. The gallery shows
one still, so `/CardProof` defined before the program (a stdin prelude, like
the `2 2 scale` other stills use) draws a 300 x 360 pt proof sheet instead:
both sides on a warm grey ground, each over a soft shadow. The manifest's
`page` stays 252x144, the card itself, which is what `pscat --page` on the
piece page draws. The site's CSS sizes the image from its own pixels, so the
proof's different aspect shows correctly.

**Whole pixels at gallery and print resolution.** #973 showed that the
decoders miss dense symbols at fractional pitches. No module size is whole at
both 144 dpi (the 2x stills) and 300 dpi, short of 6 pt. So this still is
rendered at 300 dpi, the print resolution: modules are 1.68 pt (7 px), the
symbol's corner is at (171.12, 37.44) pt, and the proof translates the cards
only by multiples of 0.24 pt (one 300 dpi pixel). The source header gives the
exact still command and says why the 2x recipe must not be used.

**Symbol.** The exact URL `https://github.com/PSCatLabs/pscat` is 34 bytes,
exactly version 4's capacity at level H. Level Q needs version 4 too, so H
costs nothing. With the four-module quiet zone, the 41-module symbol is
68.88 pt (0.96 in), above the design's 0.8 in floor. It is navy on white.
The URL is printed beside it in Courier for anyone who cannot scan.

**Design.** The paper is left white, and nothing is printed within 9 pt of
the trim, so the card needs no bleed (the design doc's "white edges"). The
front has the cat-eared `%!` logo, redrawn as paths from
`art/logo/pscat-logo-d.ps`'s geometry on its mustard tile. Beside it, the
wordmark `pscat` is in Courier-Bold, like the logo's `%!`, with the tagline
"PostScript for precision and imagination." in Palatino Italic. Under a
mustard rule, "PSCatLabs" is letterspaced on the left and "PostScript
interpreter" sits on the right. The back says "This card is a program." and
says how: every letter, rule and square, the QR code too, is drawn by a
PostScript program that pscat runs. Then comes the call to action, "Scan to
draw your own:", with the URL. Every letter is at least 9 pt, and every face
is from the LaserWriter 35 compiled into pscat. Ghostscript renders both
sides the same way (`gs -dNOSAFER`, command in the header).

**Tests (`tests/qr_card.rs`).** These run on the real gallery source:

- PDF: two `/MediaBox [0 0 252 144]` pages, `--lint-strict` clean, no image
  XObject.
- SVG: two 252 x 144 files, no `<image>`.
- PNG at 300 dpi: two 1050 x 600 files.
- Each side of each export (the SVGs through resvg, the PDF pages through
  hayro, with a new `raster_pdf_page` helper):
  - no pixel inside the 9 pt safe margin is printed;
  - on the back, every pixel of the 41 x 41 drawing, quiet zone included,
    is exactly the light or dark of its module;
  - both decoders read the exact URL at level H (rqrr: version 4) on the
    back and find nothing on the front.
- The committed still: 1250 x 1500 px, the same pixel check at the proof
  offset, both decoders, and exactly one symbol.
- A wrapped `findfont`, the show operators and the image operators prove the
  card uses only base-35 faces, sets no text under 9 pt (the em through the
  font matrix and the CTM) and paints no raster.

Each check was mutation-tested: moving the QR by one 300 dpi pixel, an 8 pt
line, a catalog face, a rule inside the margin and level Q each fail it.

**Functional primitives.** Reassessed at implementation time: `main` has no
`lib/functional.ps` or `lib/aspect.ps` (the functional branch is unmerged),
so the card uses ordinary procedures, as the design doc's decision says. It
has no optional dependency.

**Found on the way.** pscat has no `arcto` or `arct` (both Level 1/2
operators): both raise `undefined`. The card builds its rounded rectangles
from `arc`. Proposed as a follow-up.
