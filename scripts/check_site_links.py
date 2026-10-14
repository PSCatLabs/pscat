#!/usr/bin/env python3
"""check_site_links.py - link and navigation check for the built site (#250).

    scripts/check_site_links.py [SITE_DIR] [--repo REPO_ROOT]

Run it on the output of scripts/build_site.sh (default SITE_DIR: _site). It is
offline and uses only the standard library, so CI needs nothing installed.
It fails (exit 1, one line per problem) when:

  * a relative href/src in any page points at a file that is not in SITE_DIR,
    or at a #fragment that the target page does not define;
  * a link to this repo's GitHub pages (.../blob/<ref>/<path> or
    .../tree/<ref>/<path>) names a path that is not in the REPO_ROOT checkout.
    This is what catches a doc move (#249) that forgets the site;
  * a playground <option value="x.ps"> has no examples/x.ps (the 404 a
    forgotten build_site.sh copy line causes), or a link to
    playground.html?example=x.ps (the Learn page's "Try it", #252) names
    an example that is not there;
  * the files the pages load from script (pscat.js, pscat.wasm) are missing;
  * a page has no main navigation, or its navigation differs from the home
    page's, or it does not mark exactly one entry as the current page;
  * a page cannot be reached by following links from index.html (an orphan).

Other external links (http://, https://, mailto:) are not fetched: a CI check
that depends on third-party uptime goes red for reasons nobody can fix.

For #248's Markdown link checker: the helpers (repo_path_for, in_repo,
check_fragment) are plain functions, so it can import them rather than
re-implement the GitHub-URL-to-path rule.
"""
import os
import posixpath
import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

REPO_URL = "https://github.com/jeromebanks/postscript_interpreter"
# page-to-page links in these attributes
LINK_ATTRS = {"a": "href", "link": "href", "img": "src", "script": "src",
              "source": "src", "video": "src", "iframe": "src"}


class Page(HTMLParser):
    """Collects what the checks need from one HTML file."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.links = []        # (tag, url)
        self.ids = set()
        self.nav_links = []    # hrefs inside <nav aria-label="Main">
        self.nav_current = 0   # entries marked aria-current="page"
        self.options = []      # <option value=...> values
        self._in_nav = 0

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        for k in ("id", "name"):
            if a.get(k):
                self.ids.add(a[k])
        if tag == "nav" and a.get("aria-label") == "Main":
            self._in_nav = 1
        if tag in LINK_ATTRS and a.get(LINK_ATTRS[tag]):
            url = a[LINK_ATTRS[tag]]
            self.links.append((tag, url))
            if self._in_nav and tag == "a":
                self.nav_links.append(url)
                if a.get("aria-current") == "page":
                    self.nav_current += 1
        if tag == "option" and a.get("value"):
            self.options.append(a["value"])

    def handle_endtag(self, tag):
        if tag == "nav":
            self._in_nav = 0


def repo_path_for(url):
    """The repo-relative path a GitHub blob/tree URL of this repo names, or None."""
    if not url.startswith(REPO_URL + "/"):
        return None
    m = re.match(r"(?:blob|tree)/[^/]+/(.*)$", url[len(REPO_URL) + 1:])
    if not m:
        return None
    return unquote(urlsplit(m.group(1)).path).rstrip("/")


def exists_exact(root, rel):
    """True if root/rel exists with every path component spelled exactly.

    Path.exists() is case-insensitive on macOS, but Linux CI and GitHub Pages
    are not, so a link that passes locally could 404 once published.
    """
    cur = Path(root)
    for part in [p for p in rel.split("/") if p]:
        try:
            if part not in os.listdir(cur):
                return False
        except OSError:
            return False
        cur = cur / part
    return True


def in_repo(repo, rel):
    return exists_exact(repo, rel)


def check_fragment(page, frag):
    return not frag or frag == "top" or frag in page.ids


def check(site, repo):
    problems = []
    pages = {}
    for f in sorted(site.rglob("*.html")):
        p = Page()
        p.feed(f.read_text(errors="replace"))
        pages[f.relative_to(site).as_posix()] = p

    def target_file(src, path):
        """Resolve a relative url path from page src to a site-relative name."""
        base = posixpath.dirname(src)
        t = posixpath.normpath(posixpath.join(base, unquote(path)))
        if t.startswith(".."):
            return None
        if (site / t).is_dir() and exists_exact(site, t):
            t = posixpath.join(t, "index.html")
        return t

    edges = {name: set() for name in pages}
    for src, page in pages.items():
        for tag, url in page.links:
            u = urlsplit(url)
            if u.scheme in ("http", "https"):
                rp = repo_path_for(url)
                if rp is not None and not in_repo(repo, rp):
                    problems.append("%s: GitHub link to a path not in the repo: %s" % (src, url))
                continue
            if u.scheme:  # mailto:, data:, javascript:
                continue
            if not u.path:  # same-page #fragment
                if not check_fragment(page, unquote(u.fragment)):
                    problems.append("%s: no #%s on this page" % (src, u.fragment))
                continue
            t = target_file(src, u.path)
            if t is None or not exists_exact(site, t) or not (site / t).is_file():
                problems.append("%s: broken %s link: %s" % (src, tag, url))
                continue
            if t == "playground.html":
                for v in parse_qs(u.query).get("example", []):
                    if not exists_exact(site, "examples/" + v):
                        problems.append("%s: playground example %s is not in examples/" % (src, v))
            if t in pages:
                if tag == "a":
                    edges[src].add(t)
                if not check_fragment(pages[t], unquote(u.fragment)):
                    problems.append("%s: %s has no #%s" % (src, t, u.fragment))
        for v in page.options:
            if v.endswith(".ps") and not exists_exact(site, "examples/" + v):
                problems.append("%s: <option> example %s is not in examples/" % (src, v))

    for need in ("style.css", "pscat.js", "pscat.wasm"):
        if not exists_exact(site, need):
            problems.append("%s is missing from the site" % need)

    # navigation: present, identical everywhere, one current entry
    home = pages.get("index.html")
    if home is None:
        problems.append("index.html is missing")
    else:
        if not home.nav_links:
            problems.append("index.html: no <nav aria-label=\"Main\">")
        for src, page in pages.items():
            if page.nav_links != home.nav_links:
                problems.append("%s: main navigation differs from index.html's" % src)
            if page.nav_current != 1:
                problems.append("%s: %d navigation entries marked current, want 1"
                                % (src, page.nav_current))

        # orphans: everything must be reachable from index.html
        seen, todo = {"index.html"}, ["index.html"]
        while todo:
            for n in edges.get(todo.pop(), ()):
                if n not in seen:
                    seen.add(n)
                    todo.append(n)
        for name in sorted(set(pages) - seen):
            problems.append("%s: orphan page, not reachable from index.html" % name)
    return problems, len(pages)


def main(argv):
    args = argv[1:]
    repo = Path(__file__).resolve().parent.parent
    if "--repo" in args:
        i = args.index("--repo")
        repo = Path(args[i + 1])
        del args[i:i + 2]
    site = Path(args[0]) if args else repo / "_site"
    if not site.is_dir():
        print("check_site_links: %s is not a directory (run scripts/build_site.sh first)" % site,
              file=sys.stderr)
        return 2
    problems, n = check(site, repo)
    for p in problems:
        print("FAIL " + p)
    print("checked %d pages in %s: %s" % (n, site, "%d problems" % len(problems) if problems else "ok"))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
