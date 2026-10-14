# #247: docs and site audit, plan only

The audit and proposal are in `docs/dev/DOCS_PLAN.md`. Nothing was moved.

Findings worth keeping:

- `tests/headline.rs` reads a `headline.ps` snippet straight out of
  `README.md`. Slimming the README (#248) breaks that test unless the test
  is pointed at the new lettering guide in the same PR.
- `site/gallery.html` was missing 2 of the 27 gallery pieces at `1e1168e`:
  Fugitive Pigments and The Compositor's Proof. The generated gallery
  (#251) fixes this.
- The reference map was built with `git grep -n -E`, one `-e` per path.
  Two traps on macOS git: `\b` is not supported in `-E` patterns (it
  silently matched almost nothing for `VM.md`), and one big alternation
  containing a `(^|[^A-Za-z_])` group silently dropped every match under
  `lib/`, `src/` and `tests/`. Re-run it with plain `-e` patterns.
- `gallery/crayon_drawing.ps` (#240) was not on `main` yet, so it is listed
  as pending under "Drawing (dry media)".
