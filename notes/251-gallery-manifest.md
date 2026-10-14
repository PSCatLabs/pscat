# #251: gallery manifest, generated gallery and per-piece pages

## What shipped

- **`gallery/manifest.json`** is the one list of what the gallery shows. It
  holds `categories`, a `libraries` registry (library to guide), 31 `pieces`
  (the art tier) and 35 `specimens` in 8 `groups` (the specimens-and-examples
  tier). A piece has: `id`, `title`, `category`, `tags`, `libraries`,
  `featured`, `playground`, `added`, `source`, `render`, `page`, `speed`,
  `alt`, `blurb` (card text), `technique` (the page's "How it is made"), and
  optionally `issue` and `reference` (photo, captions and provenance text).
  A specimen has either a committed `render`, or a `source` plus a `page` size
  that `build_site.sh` renders with pscat.
- **`scripts/gallery.py`** (stdlib only): `check`, `sync`, `build OUT`,
  `stage OUT`, `renders`, `list`. `build` writes `gallery.html` and
  `gallery-<slug>.html` per piece with the `<!--@head-->`, `<!--@header
  gallery-->` and `<!--@footer-->` markers, so `site_pages.py` (the last build
  step) gives them the shared shell. Piece pages are flat in `_site/` because
  the shell's links are relative; a subdirectory would need a base prefix and
  would fail the nav-consistency check.
- **Page content.** The gallery has a jump bar per tier, a filter by library
  (a small script; without JavaScript every piece shows; libraries used by one
  piece only are not chips, since the piece page lists them) and a section per
  category. Each piece page has the large render, how it is made, the reference
  photo beside the render when there is one (Mochi, Lincoln, with the
  Library of Congress provenance), details (category, tags, libraries linked to
  their guides, date added, issue, page size), the source link and the command
  to run it, an "open in the playground" link for self-contained pieces, the
  first 40 lines of the source, and previous/next links.
- **Build.** `build_site.sh` runs `gallery.py check`, `build`, `stage` and
  renders the specimens that have no committed render. No hard-coded render or
  playground list is left in it. Nothing was lost from `_site/assets/renders/`
  or `_site/examples/` (compared against the old list); the eight self-contained
  pieces that were not in the playground list now are staged too.
- **Fills the gap the plan flagged**: Fugitive Pigments and The Compositor's
  Proof are on the site (31 pieces, not the plan's 27: the four pieces that
  landed since are included, Drawing is no longer empty). `mathkit_specimen`
  (on the site but absent from the plan's 32) is under Dry media, and
  `graph_paper` / `dataviz_paper` (rendered by the old build but shown nowhere)
  form a new "Charts and figures" specimen group.
- **README generated parts.** The top-level README's highlights grid is the
  `featured` pieces (the same six); `gallery/README.md`'s table, by category,
  is generated too, between markers. The stage-era intro was rewritten.
  `gallery/show.sh` reads piece, page and speed from `gallery.py list` (the
  values are identical to the old arrays).
- **Playground.** `playground.html?example=name.ps` loads a staged example
  (plain file names only). Only pieces that read no file may set `playground`;
  all twelve were run through `web/pscat.js` in node first.

## The check

`python3 scripts/gallery.py check` fails on: a tracked `gallery/*.ps` with no
entry (it uses `git ls-files`, so untracked work in progress is ignored), a
render in `gallery/renders/` that no entry uses, a missing source, render or
reference, a `page` that disagrees with `%%BoundingBox`, `libraries` that
differ from the `(lib/x.ps)` loads, a `playground` source that reads a file or
collides by name, stage or issue wording (`Stage 3`, `Gallery II`, `issue #12`)
in the text, and out-of-date generated READMEs. `tests/gallery_manifest.rs`
runs it and `scripts/test_gallery.py` under `cargo test`, so every PR is gated
on it (the `site-check` workflow is path-filtered and not required).

## Conventions changed

`AGENTS.md`, the `psart` skill, `work-issue`'s `demo-surfaces.md`, HANDOFF.md
and `docs/dev/README.md`: new pieces add a manifest entry; nobody edits
`gallery.html` or `show.sh`'s arrays.

## For later

- Open gallery tickets (#215, #238, #240, #242 and others) add a manifest entry
  instead of a card. #427 edits a README row that is now generated: its wording
  moves to the piece's `technique`.
- #249 moves docs: the guide links on piece pages are GitHub `blob` links that
  `check_site_links.py` verifies, so a move fails until the manifest's
  `libraries[*].guide` paths are updated.
- Specimens have no per-piece pages (the plan's per-piece pages are for art).

## After the merge with main (#430, #434, #436, #445, #440)

- `playground.html?example=` came from #430 (Learn's "Try it"); the gallery's
  link uses it as is (my own handler was dropped). `check_site_links.py` now
  verifies each `?example=` target exists in `_site/examples/`, which checks
  the manifest's `playground` flags end to end.
- `build_site.sh` keeps both sets of steps: the manifest-driven gallery and
  renders, and the Learn examples (`site/learn/`, `site_examples.py`).
- Manifest text follows the art changes: Alpine Lake's shadow planes are
  `pktrowelplane` and only the snow is `pktrowel` (this fixes #435), plus the
  far range's banks of haze; Evening Stream's cloud undersides are lit by
  `pkscumblein`'s `/Light`.
- New specimen: `mathkit_structures` (#440), under Dry media.
