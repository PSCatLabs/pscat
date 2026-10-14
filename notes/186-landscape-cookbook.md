## Landscape techniques cookbook (issue #186, 2026-10-14)

`docs/guides/landscape-cookbook.md`: eleven recipes for an artist agent, each a
complete snippet on a 360x240 pt page plus a 2x thumbnail in
`docs/guides/landscape/`. No `lib/` change, no landscape helper procedure.
`tests/landscape_cookbook.rs` runs every snippet.

Decisions:

- **The markdown is the single source.** The test extracts the fenced
  `postscript` blocks (first line `% recipe: <name>`) instead of reading a second
  copy from `.ps` files, so the doc and the tested code cannot drift. Snippets
  run in-process (`Interp::with_page(360, 240)`), so `within_cpu_budget` measures
  the render (a spawned `pscat` would not be counted by the thread CPU clock) and
  `Interp::lint(true)` is the in-process `--lint`. The test compares no pixels:
  thumbnails are illustrations, regenerated with `COOKBOOK_WRITE_THUMBS=1`.
- **Thumbnails at 2x (`--dpi 144`).** At 72 dpi the 1 pt speck of the vapour
  recipes reads as static; the shipped stills are 2x for the same reason. Eleven
  PNGs, about 1 MB. Total test CPU about 3.5 s in the `ci` profile (the heaviest
  recipe, evergreens, about 1 s of 2 s).
- **The pigment palette has no recipe of its own.** It is a "Colour" section
  linking `docs/paintkit_pigment_demo.png`, and the sky and bushes snippets use
  `pigcolor`.
- **Reflections: the whole-element pull is the runnable recipe**; Alpine Lake's
  record-and-replay (`lkdo`) is prose with a pointer, too big for a snippet.
- **Mist teaches the directional lift.** Alpine Lake's first mist, a stack of
  `pkbroad` strokes with falling charge, read as ruled stripes at 2x, so the
  cookbook gives it as the failure.
- **Evening Stream is cited as pending (PR #348)**, with no relative link to
  `gallery/evening_stream.ps`, which does not exist on `main` yet.

Coverage of the issue's list and #187's comment:

| Technique | Recipe |
|---|---|
| knife snow clipped to the range silhouette | 3 Mountains |
| shadow planes as clipped areas filled with pulls | 3 Mountains (#180 pending) |
| the mist fade brushed last | 4 Mist (directional lift) |
| far forest, a filled mass with a tapped crest | 5 Forest |
| reflections by re-running self-seeded procedures under `1 -1 scale` | 9 Reflections |
| framing trees on the near banks, not the far shore | 6 Evergreens, order of work |
| `pkwet /Under` is the colour behind the mark | 2 Clouds (`/At` procedure, #179), 4 Mist, 5 Forest |
| the 1 pt speck rule | 2 Clouds, 4 Mist |
| pull a whole element down by one directional `pkwet` | 9 Reflections |
| scumbled clouds: brush at least 2x the circle radius | 2 Clouds |
| a cloud lit from below | 2 Clouds |
| ruled edges broken by dabs, bushes, grass | 10 Bank, 11 Grass (#347 pending) |

Deferred: the cookbook is updated as each pending tool lands (a note on each
issue says so), and gains Evening Stream's still when PR #348 merges.
