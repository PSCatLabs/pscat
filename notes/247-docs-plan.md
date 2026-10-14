# #247: docs and site audit, plan only

The audit and proposal are in `docs/dev/DOCS_PLAN.md`. Nothing was moved.

Findings worth keeping:

- `tests/headline.rs` reads a `headline.ps` snippet straight out of
  `README.md`. Slimming the README (#248) breaks that test unless the test
  is pointed at the new lettering guide in the same PR.
- `site/gallery.html` was missing 2 of the 27 gallery pieces at `1e1168e`:
  Fugitive Pigments and The Compositor's Proof. The generated gallery
  (#251) fixes this.
- The reference map was built with `git grep -n -E -e <path> ...`, one `-e`
  per path. A single `-E` alternation that contained `(^|[^A-Za-z_])`
  silently dropped every match under `lib/`, `src/` and `tests/`, so use
  separate `-e` patterns (or `\b`) when re-running it.
- `gallery/crayon_drawing.ps` (#240) was not on `main` yet, so it is listed
  as pending under "Drawing (dry media)".
