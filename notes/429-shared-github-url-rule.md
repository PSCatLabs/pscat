# 429: one GitHub-URL-to-path rule for the link checkers

The Markdown checker (#248, `scripts/check_md_links.py`) and the site checker
(#250, `scripts/check_site_links.py`) each mapped a
`github.com/jeromebanks/postscript_interpreter/blob|tree/<ref>/<path>` URL to a
repo path. The rule now lives in `scripts/repo_url.py` (stdlib only), and both
import it.

`match(url, ref)` returns `(scheme, ref, tail)`; each checker keeps its own
query/fragment handling and scheme filter, so their answers are unchanged:

- md: `ref="main"`, http or https, `?query` kept, `#fragment` kept.
- site: any ref (`ref=None`), https only, query and fragment dropped.

Behaviour was pinned before the refactor. `SharedRepoRule` in
`scripts/test_check_md_links.py` holds the old answers for 20 URL shapes
(bare `tree/main`, `?query`, `%20`, `mainline`, `v0.5.1`, `raw`, ...). Those
rows pass against the old checkers and the new ones. A differential run over
every repo URL in the tracked Markdown and the built `_site` (145 URLs) gave
identical output before and after.

The md rule's ref prefix matched `main` without a boundary, so
`blob/mainline/x.md` resolved to `line/x.md`. Fixed in #481: the ref must now
be followed by `/`, `?`, `#` or the end of the URL.

`scripts/repo_url.py` is in the `site-check.yml` path filter. The md tests
run in `links.yml`, which has no path filter.
