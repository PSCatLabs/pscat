# Contributor docs

Notes for people working on pscat itself. For conventions start with
[`AGENTS.md`](../../AGENTS.md) and [`SDLC.md`](../../SDLC.md); for a tour of the
code, [`HANDOFF.md`](../../HANDOFF.md) and [`ARCHITECTURE.md`](../../ARCHITECTURE.md).
[`DOCS_PLAN.md`](DOCS_PLAN.md) records how the documentation is organized.

## Previewing the website

The project site is at
**[jeromebanks.github.io/postscript_interpreter](https://jeromebanks.github.io/postscript_interpreter/)**:
architecture and extending guides, the full art/font/example gallery, a
dedicated [font gallery](https://jeromebanks.github.io/postscript_interpreter/fonts.html)
(every Type 3 face and every catalog family, one card each), and a live
playground running the wasm interpreter, all in the browser, no install.

It is published from this repo by `.github/workflows/pages.yml` (GitHub Pages,
"GitHub Actions" source) on every push to `main`. The sources are in `site/`.
To preview locally:

```sh
./scripts/build_site.sh                  # assembles _site/
python3 -m http.server -d _site          # open http://localhost:8000
```

`build_site.sh` also renders the font gallery cards
(`scripts/build_font_gallery.sh`) and the example and gallery stills fresh
each time, so the site never drifts from what the interpreter actually does.
