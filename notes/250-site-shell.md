# #250: site shell, landing page, navigation, link check

## What shipped

- **Shared shell.** `site/_partials/{head,header,footer}.html` hold the markup
  every page shares. A page carries `<!--@head-->`, `<!--@header KEY-->` and
  (optionally) `<!--@footer-->`; `scripts/site_pages.py` expands them in
  `_site/` as the **last** step of `scripts/build_site.sh`, so pages that a
  later step generates (the gallery in #251, Learn in #252) get the shell for
  free. KEY is the nav slot highlighted as current. A page with no head or
  header marker, an unknown KEY, or a leftover marker fails the build.
- **Navigation** is the plan's nine items (DOCS_PLAN section 2.5): Home, Learn,
  Gallery, Fonts, Playground, Guides, For agents, Contribute, GitHub. Charts and
  JavaScript sit under Guides; Architecture and Extending under Contribute.
  No page was renamed, so old links still work.
- **New pages.** `guides.html` (cards for the interactive pages and the written
  guides, each Markdown guide linked on GitHub), `contribute.html`, and
  `learn.html`, a placeholder that #252 replaces (it already has the nav slot).
  `guides.html` links the README sections that #248 will split into
  `docs/guides/*.md`; #248 should repoint those cards.
- **Style.** One `style.css` with dark and light themes (follows
  `prefers-color-scheme`; every colour is a token), sans-serif prose with
  monospace kept for code and navigation, and a phone layout (wrapping,
  non-sticky nav; the playground stacks the editor over the canvas).
- **Newcomer pass** over the pages touched: removed "Stage 12" and "stage by
  stage" wording and the ROADMAP pointer, fixed a "See Handwrite" link that
  pointed at the home page, spelled out PLRM, and corrected `agents.html` (the
  MCP server has four tools, not three; `describe_art_capabilities` was
  missing). `gallery.html` keeps its "Stage 3" wording: #251 regenerates it.

## Link check

`scripts/check_site_links.py [_site]` is offline and stdlib-only. It checks
relative links and fragments, GitHub `blob`/`tree` links against the checkout
(so #249's doc moves fail the site check until the site is updated), playground
`<option>` examples, navigation consistency, and orphan pages.
`scripts/test_site_scripts.py` is its self-test.

- `pages.yml` runs it between the build and the upload, so a broken link blocks
  the deploy.
- `.github/workflows/site-check.yml` runs the build and the check on pull
  requests that touch anything the site is built from. It is a separate
  workflow because `pages.yml`'s `pages` concurrency group cancels in-flight
  runs (a PR run must not cancel a deploy), and a skipped deploy job would show
  up as `SKIPPED` in `statusCheckRollup`. It is not a required check.
- #248's Markdown link checker can import `repo_path_for` / `in_repo` instead of
  re-implementing the GitHub-URL-to-path rule, or run alongside this one.

## Left for others

- #251: generated `gallery.html` (keep the `<!--@head-->`, `<!--@header gallery-->`
  and `<!--@footer-->` markers, or generate them before the final pass).
- #252: replace `learn.html`.
- #249: after the doc moves, update the GitHub links in `site/*.html`; the check
  will list every one.
