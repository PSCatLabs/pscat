# Demo surfaces (step 5)

Read by `SKILL.md` step 5 when the change has something a human could
look at.

- If the feature has something a human could actually look at (a new
  art capability, template, chart type, font, etc.), give it a demo
  somewhere it's actually seen — and match the surface's real
  requirements, not just its most visible file:
  - A generative-art piece: add it to `gallery/show.sh`'s `PIECES`
    array *and* the parallel `PAGES`/`SPEEDS` arrays at the same
    index — all three are indexed together under `set -u`, so a
    `PIECES`-only addition leaves `--live` mode aborting on an unbound
    variable the moment it reaches the new entry. Also update
    `gallery/README.md`, *and* render + commit
    `gallery/renders/<name>.png` — `show.sh`'s default (non-`--live`)
    mode silently skips any piece missing that PNG, which defeats the
    whole point.
  - The published site: a card in `site/gallery.html`, backed by a
    PNG in `_site/assets/renders/`. For a gallery piece, that PNG
    comes free from `build_site.sh`'s existing
    `gallery/renders/*.png` wildcard copy — don't *also* add a
    `render` call targeting the same basename, since `render` runs
    after that copy and would overwrite the committed, intentionally
    2×-supersampled still with a plain canonical-size one
    (`gallery/README.md`'s "Re-rendering the stills" explains the
    supersampling). A `render` call in `scripts/build_site.sh` is for
    anything *without* a pre-rendered gallery still — most
    `examples/*.ps` pieces. An `<option>` in `site/playground.html`'s
    picker additionally requires the file be self-contained — no
    `(lib/x.ps) run` of a sibling library, since the wasm build has no
    filesystem — and copied into `build_site.sh`'s own source loop, or
    the playground fetches a 404.

  - A library specimen (a `lib/` file's own showcase, e.g. a brush or paper kit): a
    committed PNG under `docs/` (next to the writeup it illustrates, in that doc's
    subdirectory) is the required demo, rendered from the piece's `.ps` at the size the doc
    shows it. A card in `site/gallery.html` (with its PNG in `_site/assets/renders/`, per
    the rules above) is optional; skip it unless the issue asks for one.

  Copy an existing similar entry's pattern rather than guessing which
  of these apply. An `examples/*.ps` file alone is a regression
  fixture, not a demo; it's fine as the *only* artifact when the
  feature genuinely has no visible surface (an internal fix, a lint
  check, a Rust-only refactor), but don't let that be the default
  reason to skip this.
