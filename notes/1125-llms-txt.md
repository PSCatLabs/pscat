# #1125: site/llms.txt

`site/llms.txt` follows llmstxt.org (an H1, a `>` summary, a paragraph of
install steps, H2 sections of `- [title](url): description` links, and an
`## Optional` section). `scripts/build_site.sh` copies it to the site root, so it
is served at `https://pscatlabs.github.io/pscat/llms.txt`. The site footer and
every page's `<head>` (`<link rel="alternate">`) point at it, and
`docs/guides/agents.md` mentions it.

- **Links.** Guides and skills link to their Markdown on
  `raw.githubusercontent.com/PSCatLabs/pscat/main/...` (the convention prefers
  Markdown to HTML); pages that exist only as HTML link to the site.
- **Check.** `check_site_links.py` (run by `site-check.yml` and `pages.yml`) now
  also checks `llms.txt`: the shape, every site, raw and GitHub link against the
  built site or the checkout (case-exact), and that every `docs/guides/*.md` is
  listed, so a new guide that is not added fails the PR. External links
  (crates.io, GitHub releases) are not fetched, as for the pages. The
  self-test is in `scripts/test_site_scripts.py`.
- **No `llms-full.txt`.** The guides are about 325 KB of Markdown; one file of
  that is too big for most contexts and would go stale with every guide edit.
  Agents can fetch the one guide the job needs from the list.
- **Not covered.** The site has no `robots.txt` or sitemap, so there is nothing
  to list it in. The hosted MCP server (epic #689) has no URL yet, so `llms.txt`
  says only that one is planned; the client and route matrix is #1136.
- **Live only after merge.** `llms.txt` itself, and the `agents.md` link to it,
  return 404 until the Pages deploy after the merge. The raw links to existing
  files resolve today; the check accepts only a `main` ref and validates the
  paths against the checkout.
