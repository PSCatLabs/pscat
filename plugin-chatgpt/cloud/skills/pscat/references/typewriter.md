<!-- cloud: replace "## Typewritten pages (text → PNG/PDF)" -->
## Typewritten pages (text → PNG/PDF)

There is no typewriter tool here. Typed pages are written in PostScript and
rendered with `render_postscript` (`format` `png`, `svg` or `pdf`; SVG pages are
large, so prefer PNG or PDF).

Load the `/Typewriter` font with `(lib/typewriter_outlines.ps) run
(lib/typewriter.ps) run`, then pass one options dict to `tw-type` and end the
page:

`<< /Text (Dear Sir,\n\nThe carbons are attached.) /Seed 3 >> tw-type showpage`

The text wraps by itself, with per-key quirks, carriage drift, per-strike
variation and ribbon ink. Other keys: `/Width`, `/Height` and `/Margin`,
`/Spacing` (1.5 is a line and a half), `/Machine` and `/Seed` (the same machine
and seed give the same page), `/PageNumbers /bottom`, `/PageStart { ... }` to
paint the paper first, and `/Memo << /To (..) /From (..) /Subject (..) /Date
(..) >>` for a memo header. Markup in the text: `<u>underlined</u>`,
`<x>crossed out</x>`, `<o>old|new</o>`, `<r>redacted</r>`. `tw-linecount` and
`tw-pageheight` size a page to its text without drawing it. Non-ASCII becomes
ASCII or `?`. Longer text makes more pages.
