#!/usr/bin/env python3
"""site_pages.py - apply the shared site shell (header, footer, <head>) to pages.

    scripts/site_pages.py DIR [PARTIALS_DIR]     # rewrite every DIR/*.html in place

A page carries three markers instead of a copy of the shared markup:

    <!--@head-->            charset, viewport, icon and stylesheet (inside <head>)
    <!--@header KEY-->      the top navigation; KEY is the nav slot this page
                            belongs to (home, learn, gallery, fonts, playground,
                            guides, agents, contribute), highlighted as "here"
    <!--@footer-->          the site footer (optional: the full-height
                            playground has none)

The partials live in site/_partials/ (head.html, header.html, footer.html);
PARTIALS_DIR overrides that. build_site.sh copies the pages into _site/ and
runs this over _site/ as its last pass, so pages that a build step generates
(the gallery, #251) get the shell too. Run it once per tree: a page that was
already expanded has no markers left and is rejected.
The build fails on a page with no head or header marker, an unknown nav key,
or a marker left unexpanded: a page can't silently ship without the nav.
"""
import re
import sys
from pathlib import Path

MARKER = re.compile(r"<!--@(head|header|footer)(?:\s+([a-z-]+))?\s*-->")


def load_partials(partial_dir):
    return {k: (partial_dir / f"{k}.html").read_text() for k in ("head", "header", "footer")}


def nav_keys(header_html):
    return re.findall(r'data-nav="([a-z-]+)"', header_html)


def mark_here(header_html, key):
    """Highlight the link whose data-nav equals key."""
    pat = re.compile(r'<a data-nav="%s"' % re.escape(key))
    if not pat.search(header_html):
        raise ValueError("no nav slot %r (have: %s)" % (key, ", ".join(nav_keys(header_html))))
    return pat.sub('<a data-nav="%s" class="here" aria-current="page"' % key, header_html, count=1)


def apply_shell(text, partials, name):
    seen = set()

    def repl(m):
        kind, arg = m.group(1), m.group(2)
        seen.add(kind)
        if kind == "head":
            return partials["head"].rstrip("\n")
        if kind == "footer":
            return partials["footer"].rstrip("\n")
        if not arg:
            raise ValueError("%s: <!--@header--> needs a nav key" % name)
        return mark_here(partials["header"], arg).rstrip("\n")

    out = MARKER.sub(repl, text)
    for need in ("head", "header"):
        if need not in seen:
            raise ValueError("%s: missing <!--@%s--> marker" % (name, need))
    if "<!--@" in out:
        raise ValueError("%s: unexpanded marker left in page" % name)
    return out


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(argv[1])
    default = Path(__file__).resolve().parent.parent / "site" / "_partials"
    partials = load_partials(Path(argv[2]) if len(argv) == 3 else default)
    n = 0
    for page in sorted(out.glob("*.html")):
        page.write_text(apply_shell(page.read_text(), partials, page.name))
        n += 1
    print("site shell applied to %d pages" % n)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (ValueError, OSError) as e:
        print("site_pages: " + str(e), file=sys.stderr)
        sys.exit(1)
