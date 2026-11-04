# gallery.py: image width/height come from the still, not the page (#984, 2026-11-03)

`scripts/gallery.py` wrote each thumbnail's and figure's `width`/`height` from the manifest `page`.
A still whose shape differs from its page (`gallery/pscat_card.ps`: page 252x144, but the still is a
two-sided proof sheet of 1250x1500 px, 5:6) made the browser reserve the wrong box, so the lazy-loaded
thumbnail jumped.

- `png_size()` reads the pixel size from the PNG's IHDR chunk with `struct` (no Pillow); `thumb_size()`
  applies the rule below. A missing or non-PNG render stops the build with the piece's id in the message.
- A still with its page's shape (to within half a percent) keeps the page size as its attributes. Most
  stills are 2x their page and one (`hortus`) is 1x, so no single divisor reproduces today's markup;
  matching the shape does. Every other still is shown at its pixel size at the gallery's 2x scale
  (`STILL_SCALE`), so the card is 625x750. `page` stays the physical page in the "Page size" fact and
  in the source link.
- Building the site before and after changes exactly two lines: the `pscat_card` card in `gallery.html`
  and its figure in `gallery-pscat-card.html`. `gallery.py sync` writes no image attributes (the README
  highlights use percentage widths), so it changes no tracked file.
- On the piece page `figure.piece img` is `width: auto; max-height: 86vh`, so the card's figure is now
  taller (750 px, capped by the viewport) than the 144 px it was.
- A still at 300 dpi is assumed to be shown at the 2x scale. Once the PNG writer records its resolution
  (pHYs, #982), `thumb_size` could use that instead of the fixed 2.
