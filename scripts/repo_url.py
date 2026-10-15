"""The one rule for GitHub URLs into this repo, shared by the link checkers (#429).

    from repo_url import match

`match(url, ref)` splits a `github.com/jeromebanks/postscript_interpreter/
blob|tree/<ref>/<tail>` URL into `(scheme, ref, tail)`, or returns None. The
tail is the raw text after the ref, so `?query` and `#fragment` are still on it:
each checker strips them the way it always has (check_md_links.py keeps the
fragment, check_site_links.py drops it). Standard library only.
"""
import re

REPO_SLUG = "jeromebanks/postscript_interpreter"
REPO_URL = "https://github.com/" + REPO_SLUG

_PREFIX = r"(https?)://github\.com/" + re.escape(REPO_SLUG) + r"/(?:blob|tree)/"


def match(url, ref=None):
    """Return (scheme, ref, tail) for a blob/tree URL into this repo, else None.

    ref=None accepts any ref and needs a "/" after it. A given ref is matched
    literally with an optional "/" after it, so ref="main" also matches
    "blob/mainline/x" with tail "line/x". The checkers keep that behaviour.
    """
    if ref is None:
        m = re.match(_PREFIX + r"([^/]+)/(.*)", url)
    else:
        m = re.match(_PREFIX + "(" + re.escape(ref) + r")/?(.*)", url)
    if not m:
        return None
    return m.group(1), m.group(2), m.group(3)
