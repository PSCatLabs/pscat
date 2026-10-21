# Demo surfaces (step 5)

Read by `SKILL.md` step 5 when the change has something a human could
look at.

- If the feature has something a human could actually look at (a new
  art capability, template, chart type, font, etc.), give it a demo
  somewhere it's actually seen — and match the surface's real
  requirements, not just its most visible file:
  - A generative-art piece: add an entry to `gallery/manifest.json` (copy a neighbour;
    `gallery/README.md` "Adding a piece" lists the fields), render + commit
    `gallery/renders/<name>.png`, then run `python3 scripts/gallery.py sync` and
    `python3 scripts/gallery.py check`. **The manifest is the only list**: the
    site's `gallery.html` and the per-piece pages, the README highlights, the
    `gallery/README.md` table and `gallery/show.sh` (which reads each piece's page
    size and speed from it) are generated from it, so never edit them by hand and
    never add the piece to `show.sh` or `build_site.sh`. `cargo test` runs the
    check: a tracked `gallery/*.ps` with no entry, a missing render, or a stale
    generated table fails it. Record the exact `gs` invocation the piece needs in its
    header comment: a piece that reads a file (`(lib/artkit.ps) run`) needs `-dNOSAFER` or
    `--permit-file-read=lib/` (the trailing slash is required; `lib` without it still fails) plus relative paths, and gs run in the default safe mode fails it
    with `invalidfileaccess`. Set `"playground": true` only if the file reads no other
    file (the wasm build has no filesystem); the build then stages it in
    `_site/examples/` and the piece's page gets an "open in playground" link. The
    renders are 2x supersampled (`gallery/README.md`, "Re-rendering the stills"): the
    manifest's render is the committed still, and `build_site.sh` never re-renders it.
  - The published site, for a specimen or example that is not a gallery piece: a
    `specimens` entry in `gallery/manifest.json` (a `render` committed in the repo, or a
    `source` plus a `page` size that `build_site.sh` renders with pscat). Do not edit
    `site/gallery.html`: it no longer exists.

  - A library specimen (a `lib/` file's own showcase, e.g. a brush or paper kit): a committed
    PNG under `docs/` (the specimen render) plus an optional `specimens` entry in
    `gallery/manifest.json` per the rules above.

  A specimen or render change is a visible change: it merges as tier (b) after the perceptual review, or
  stops for the owner as tier (c) (perceptual-review.md, #674). Either way the images are pinned as below.
  A specimen PR that stops at the no-merge path (pitfalls.md "Stop without merging") must **commit
  its render and embed it** (`![...](docs/<name>.png)` in the PR body or a comment, with an absolute
  `https://github.com/<owner>/<repo>/blob/<head sha>/docs/<name>.png?raw=true` URL), pinned to the
  head SHA and rendered **after the final rebase**: a render taken before it can show a library
  that is no longer the one in the PR (#286 had none, #303). Re-render and re-push if the rebase
  moved `lib/`.
  Merging `main` into the branch after you pinned the image URLs (a `DIRTY` resolution, 8.12) moves
  the head: the pinned SHA is then stale, and a stale SHA still renders, so nobody notices. Re-pin
  every URL to the new head and update the body with `gh pr edit <PR#> --body-file <file>`.

  Copy an existing similar entry's pattern rather than guessing which
  of these apply. An `examples/*.ps` file alone is a regression
  fixture, not a demo; it's fine as the *only* artifact when the
  feature genuinely has no visible surface (an internal fix, a lint
  check, a Rust-only refactor), but don't let that be the default
  reason to skip this.
