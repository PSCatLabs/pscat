# #461: a "View source" link on every gallery image

## What shipped

- **Every picture has a View source link**: each art card, each specimen card
  and each piece page (in its Source section, above the first-40-lines
  preview). It opens the whole program in a modal `<dialog>`.
- **`site/source-viewer.js`** (no dependencies) fetches the staged file when the
  window opens, so the gallery page carries no source. The window has a header
  (file name, line count, size, "On GitHub", Close, and the library files the
  program runs, linked to GitHub), and a scroll area with a sticky line-number
  gutter. Long lines scroll sideways inside the window, so the layout never
  breaks. The page behind is locked (`html.dialog-open`). Esc, Close and a
  click on the backdrop close it, focus goes to the scroll area on open (so
  the arrow keys scroll at once) and back to the link on close. A late fetch for
  an earlier open is dropped. If `<dialog>` is missing or JavaScript is off, the
  link is a plain link to the staged file. Light and dark follow the site
  tokens; at phone width the window fills the screen.
- **Staging.** `gallery.py stage` copies each entry's source to
  `_site/source/<repo path>` (for example `source/gallery/alpine_lake.ps`), the
  same path as in the repository, so two files with one name cannot collide.
  Same-origin, so it works on GitHub Pages.
- **Data comes from the generator.** `gallery.py` writes the file name, line
  count, byte size, GitHub URL and the loaded library files into data
  attributes on the link, so any manifest entry (a new piece from another
  ticket, say) gets the feature with no extra work. Line count rule: split on
  `\n`, ignoring the newline that ends the file; the JS uses the same rule.
- **Checks.** `gallery.py check` now fails when an entry's `source` is not a
  single file, or when it runs a `(lib/x.ps) run` that does not exist (the
  window's library links live in data attributes, which the link checker does
  not read). `check_site_links.py` fails when a card in `gallery.html` or a
  `gallery-*.html` page has no View source link, when one does not point into
  `source/`, and (as for any relative link) when the staged file is missing.
  `tests/site.rs` also compares every staged file with the repository's.

## Manifest change

Four font specimens (`font_library`, `font_library2`, `font_catalog1`,
`font_catalog2`) had a directory (`lib/fonts`, `fonts/catalog`) as `source`, so
there was no single program to show. They now name the programs that draw them:
`examples/font_library.ps`, `examples/font_library2.ps` and
`examples/font_catalog.ps` (both catalog sheets; one file draws two pages).
Their GitHub link is the program, not a folder of fonts.

## Measured

The longest source is `gallery/alpine_lake.ps` (661 lines, 27.7 KB): the window
opens at once and scrolls to the end in one frame (headless Chromium, desktop
and 390 px wide). No horizontal page scroll at either width.

## Not done

No syntax highlighting, no library inlining, no copy button (the issue made the
first two optional or excluded).
