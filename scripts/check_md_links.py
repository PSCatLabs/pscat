#!/usr/bin/env python3
"""Check the links in the repo's tracked Markdown files.

    python3 scripts/check_md_links.py            # check everything
    python3 scripts/check_md_links.py README.md docs/guides/painting.md

What it checks, for every tracked `*.md` file (except the skip list below):

* relative links and images (`[text](path)`, `![alt](path)`, reference
  definitions, and `<a href>` / `<img src>` tags) point at a tracked file or
  directory, matched case-sensitively the way GitHub's renderer matches them;
* `#fragment`s on Markdown targets (and on same-file links) name a real
  heading, using GitHub's slug rule (lower-case, punctuation dropped,
  spaces to hyphens, `-1`/`-2` for repeated headings), or an explicit
  `id=`/`name=` anchor;
* absolute links into this repo (`github.com/PSCatLabs/pscat/
  blob|tree/main/...`) resolve against the checkout, so a moved file is caught
  before the link goes live.

What it does not check: external URLs (they fail for reasons that have nothing
to do with the commit, which makes a CI gate flaky), links to the published
site's pages (the site's own checker, `scripts/check_site_links.py`, covers
`_site/`), and fenced or inline code, where paths are examples, not links.

Targets come from `git ls-files`, not the filesystem, so an untracked file, a
`target/` build directory, or a case-insensitive filesystem cannot hide a break.

Exit status is 0 when clean, 1 when any link is broken.
"""
import re
import subprocess
import sys
from pathlib import PurePosixPath
from urllib.parse import unquote

from repo_url import match as repo_match  # the GitHub-URL rule, shared with check_site_links.py

# GitHub's own `#L10` / `#L10-L20` line anchors on file views.
LINE_ANCHOR = re.compile(r"^L\d+(?:C\d+)?(?:-L\d+(?:C\d+)?)?$")

# Files that are never checked. History is kept as written, so its links may be
# stale, and notes/ fragments are per-issue working logs.
SKIP_PREFIXES = ("docs/archive/", "notes/")


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files", "-z"], check=True, capture_output=True, text=True
    ).stdout
    return [p for p in out.split("\0") if p]


def _blank(m):
    return re.sub(r"[^\n]", " ", m.group(0))


def blank_fences(text):
    """Blank out fenced code blocks, keeping line numbers."""
    return re.sub(r"^(```|~~~).*?^\1[^\n]*$", _blank, text, flags=re.S | re.M)


def blank_code(text):
    """Blank out fenced blocks and inline code spans, keeping line numbers."""
    return re.sub(r"`[^`\n]*`", _blank, blank_fences(text))


def slugify(heading):
    """GitHub's heading anchor for one heading's text."""
    s = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)  # links keep their text
    s = re.sub(r"<[^>]+>", "", s)  # inline html
    s = s.replace("`", "").replace("*", "")
    s = s.strip().lower()
    s = re.sub(r"[^\w\- ]", "", s)  # drop punctuation (unicode letters stay)
    return s.replace(" ", "-")


def anchors_of(text):
    """The set of fragment names a Markdown file offers."""
    anchors, seen = set(), {}
    stripped = blank_fences(text)  # headings keep their `code` text in the slug
    for m in re.finditer(r"^ {0,3}#{1,6}[ \t]+(.+?)[ \t]*#*[ \t]*$", stripped, flags=re.M):
        slug = slugify(m.group(1))
        n = seen.get(slug, 0)
        seen[slug] = n + 1
        anchors.add(slug if n == 0 else f"{slug}-{n}")
    for m in re.finditer(r'<[a-zA-Z][^>]*?\b(?:id|name)="([^"]+)"', stripped):
        anchors.add(m.group(1))
    return anchors


LINK_PATTERNS = [
    re.compile(r"\]\(\s*<?([^)\s>]+)>?(?:\s+(?:\"[^\"]*\"|'[^']*'))?\s*\)"),
    re.compile(r"^ {0,3}\[[^\]]+\]:\s*<?(\S+?)>?(?:\s+.*)?$", re.M),
    re.compile(r"""\b(?:href|src)\s*=\s*["']([^"']+)["']"""),
]


def extract_links(text):
    """Yield (line_number, target) for every link-like target in the text."""
    stripped = blank_code(text)
    for pat in LINK_PATTERNS:
        for m in pat.finditer(stripped):
            line = stripped.count("\n", 0, m.start(1)) + 1
            yield line, m.group(1)


def resolve(src, target):
    """Turn a link target into (repo_path or None, fragment). None = skip it."""
    m = repo_match(target, ref="main")
    if m:
        path, _, frag = m[2].partition("#")
        return unquote(path.rstrip("/")), frag
    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("//"):
        return None, ""  # external: mailto:, https:, ...
    path, _, frag = target.partition("#")
    path = path.partition("?")[0]
    if not path:
        return src, frag  # same-file fragment
    path = unquote(path)
    base = PurePosixPath(path.lstrip("/")) if path.startswith("/") else PurePosixPath(src).parent / path
    parts = []
    for part in base.parts:
        if part == "..":
            if not parts:
                return "../" + path, frag  # escapes the repo
            parts.pop()
        elif part != ".":
            parts.append(part)
    return "/".join(parts), frag


def check(files_to_check=None, tracked=None, read=None):
    tracked = tracked if tracked is not None else tracked_files()
    tracked_set = set(tracked)
    dirs = {""}
    for p in tracked:
        d = PurePosixPath(p).parent
        while str(d) != ".":
            dirs.add(str(d))
            d = d.parent
    read = read or (lambda p: open(p, encoding="utf-8").read())
    md = [p for p in tracked if p.endswith(".md")]
    if files_to_check:
        md = [p for p in md if p in set(files_to_check)]
    md = [p for p in md if not p.startswith(SKIP_PREFIXES)]

    anchor_cache = {}

    def anchors(path):
        if path not in anchor_cache:
            anchor_cache[path] = anchors_of(read(path))
        return anchor_cache[path]

    errors = []
    for src in md:
        for line, target in extract_links(read(src)):
            path, frag = resolve(src, target)
            if path is None:
                continue
            if path.startswith("../"):
                errors.append(f"{src}:{line}: link leaves the repo: {target}")
                continue
            if path in tracked_set:
                if (
                    frag
                    and path.endswith(".md")
                    and not LINE_ANCHOR.match(frag)
                    and unquote(frag).lower() not in anchors(path)
                ):
                    errors.append(f"{src}:{line}: no heading '#{frag}' in {path} (link: {target})")
            elif path in dirs:
                pass  # a directory: GitHub renders its listing (or README)
            else:
                errors.append(f"{src}:{line}: broken link to {path or '.'} (link: {target})")
    return errors, len(md)


def main(argv):
    errors, n = check(argv[1:] or None)
    for e in errors:
        print(e)
    print(f"checked {n} Markdown files: {len(errors)} broken link(s)")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
