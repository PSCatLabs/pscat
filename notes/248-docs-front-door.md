## README slimmed to a front door; guides and archive created (issue #248, 2026-10-14)

`README.md` went from 1,610 lines to about 100: what pscat is, a six-render hero
grid linked to the site gallery, a quick start (build, render, REPL taste), a
guide list, agents, contributing, the "Why" paragraph and the `sdlcify` block.
The plan is `docs/dev/DOCS_PLAN.md` (#247); this is its first step.

**Where the old README went** (section dispositions follow the plan):

- "Status" (stage log) to `docs/archive/STATUS_LOG.md`, verbatim under a short
  provenance heading; `docs/archive/README.md` indexes it. The whole old README
  is also kept word for word as `docs/archive/README_BEFORE_FRONT_DOOR.md`,
  because the guides were edited (issue references and build-note asides
  removed) and "nothing is lost" should hold for the prose too.
- "Building & running" and "Installing pscat without the repo" to
  `docs/guides/getting-started.md`, with the sweep, spool, halftone and PDF
  prose from the stage log folded in as "more on the less obvious flags".
- "Handwrite" to `handwriting.md`; "Typewriter", "The font library" and "The
  font catalog" to `fonts.md`.
- "Making art" was one unbroken run of library paragraphs. Split by library:
  `artkit.md`, `style-packs-and-templates.md` (style packs and pagekit),
  `charts.md` (graph, dataviz), `painting.md` (paintkit, now with a heading per
  brush), `printmaking.md` (etching, hatchkit, stipplekit, halftonekit,
  surfacekit, printkit), `lettering.md` (lettering, fillkit, apparel, headline,
  jersey), `dry-media.md` (drykit, drychalk, drycrayon, drycharcoal,
  strokeglyphs, mathkit).
- "In the browser" to `browser.md`; "For agents" to `agents.md`.
- "The website" local preview to `docs/dev/README.md` (a stub that #249 extends
  into the contributor index).
- "Gallery": collapsed into the hero grid and a link to `gallery/README.md`,
  which already held the full piece table and the re-render instructions.

**Light editing in the guides.** `(issue #N)` asides and stage references are
gone, "sibling" ordinals ("a fifth sibling, ...") are gone, the artkit paragraph
became a bulleted list, the first example moved to the top of each guide where
there was one, and build-note passages (what was rendered first and rejected)
were folded to the conclusion. A truncated sentence in the old apparel paragraph
was repaired. `drycrayon` and `drycharcoal` had no README text; each got a short
paragraph written from its header comment. The `/Typewriter` paragraph about
what #212 and #213 will add was dropped from the guide (still in the archived
README): forward-looking progress narrative that the library header owns.

**Load-bearing fixes.** `tests/headline.rs` now reads its snippet from
`docs/guides/lettering.md` (kept byte-identical). `AGENTS.md`, the `pscat` skill,
the `work-issue` skill (step 5.1, rationale.md) and `pscat_mcp.rs` no longer
point at README sections that are gone.

**Link check.** `scripts/check_md_links.py` (stdlib only, run by
`.github/workflows/links.yml` on ubuntu) checks every tracked Markdown file
except `docs/archive/` and `notes/`: relative links and images, `<a href>` and
`<img src>`, reference definitions, `#fragments` against GitHub's heading-slug
rule, and `github.com/.../blob|tree/main/...` links against the checkout.
Targets come from `git ls-files`, so the check is case-sensitive like GitHub's
renderer and ignores untracked files. It deliberately does not fetch external
URLs (flaky) or published-site pages (`scripts/check_site_links.py`, #250,
covers `_site/`). `scripts/test_check_md_links.py` pins the slugger and the
rules.

**Left for later.** Image links in `painting.md` point at `../paintkit_*.png`;
#249 moves `paintkit_loading_demo.png` and `paintkit_canvas_demo.png` into
`docs/dev/design/`, and the link check will fail on that PR until it repoints
them (that is the check doing its job). `fonts.md`, `agents.md` and `README.md`
link root files (`HANDOFF.md`, `ARCHITECTURE.md`, `CAPABILITIES.md`) that #249
moves; same deal. `README.md` links the site's Learn page, which #252 fills in.
