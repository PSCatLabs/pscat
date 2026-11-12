#!/usr/bin/env python3
"""Which independent review does this PR need: cross-family (Codex) or agent?

SDLC.md's `independent_review.cross_family_paths` lists the paths where a bug
costs the most: engine and library code, the npm runtime and its build,
dependencies, the files that decide what runs on users' machines, the release
scripts and the workflows that publish. A PR that touches one of them gets a
cross-family review (Codex). Every other PR (docs, skills, PostScript
libraries, gallery, tests, CI) gets the same-family blank-context agent review.
The work-issue skill runs this script instead of eyeballing globs:

    python3 scripts/review_tier.py --base origin/main   # PR vs main
    python3 scripts/review_tier.py path/a path/b        # explicit paths
    python3 scripts/review_tier.py --check-paths        # entries exist

It prints `cross-family` and then each matching path, indented, or `agent`.
Exit status 0 either way, 1 when `--check-paths` finds an entry that matches
no tracked file, 2 on a usage, git or parse error. It fails closed: when no
copy of SDLC.md yields a list, that is exit 2, never `agent`.

The patterns use CODEOWNERS syntax (`scripts/codeowners_check.py`). They are
read from BOTH the base's SDLC.md and the working tree's copy (found from the
repository root, whatever the current directory), and a path matching either
one counts, so a PR cannot lower its own tier by editing the list. This script
and codeowners_check.py are themselves in the list and in CODEOWNERS, so a PR
cannot lower its tier by editing them either.
"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from codeowners_check import changed_files, git, to_regex  # noqa: E402

SDLC = "SDLC.md"
BLOCK = "independent_review:"
KEY = "cross_family_paths:"


def patterns(text):
    """The `independent_review.cross_family_paths` list from SDLC.md's text.

    Returns None when the frontmatter has no `independent_review` block. Raises
    ValueError when the block is there but has no usable list, so a typo fails
    closed instead of sending every PR to the agent tier. The list is one line,
    a JSON array of strings (a trailing `# comment` is allowed), so no YAML
    parser is needed."""
    lines = text.lstrip("﻿").splitlines()
    if not lines or lines[0].strip() != "---":
        return None
    in_block = seen_block = False
    for line in lines[1:]:
        if line.strip() == "---":
            break
        if not line.strip() or line.lstrip().startswith("#"):
            continue  # blank lines and comments never end a block
        if not line.startswith(" "):
            in_block = line.split("#", 1)[0].replace(" ", "") == BLOCK
            seen_block |= in_block
            continue
        if in_block and line.startswith("  ") and not line.startswith("   ") \
                and line.strip().startswith(KEY):
            rest = line.strip()[len(KEY):].strip()
            try:
                value, end = json.JSONDecoder().raw_decode(rest)
            except json.JSONDecodeError as e:
                raise ValueError(f"{SDLC}: {KEY} is not a JSON list ({e})") from e
            tail = rest[end:].strip()
            if tail and not tail.startswith("#"):
                raise ValueError(f"{SDLC}: unexpected text after {KEY} list: {tail}")
            if not isinstance(value, list) or not value \
                    or not all(isinstance(p, str) and p for p in value):
                raise ValueError(f"{SDLC}: {KEY} must be a non-empty list of strings")
            return value
    if seen_block:
        raise ValueError(f"{SDLC}: independent_review has no {KEY} line")
    return None


def matches(pattern_sets, paths):
    """The paths that match any pattern in any set, in input order."""
    regexes = [to_regex(p) for ps in pattern_sets for p in ps]
    return [p for p in paths if any(r.match(p) for r in regexes)]


def unmatched(pattern_list, tracked):
    """The patterns that match no tracked file (a typo matches nothing)."""
    return [p for p in pattern_list if not any(to_regex(p).match(t) for t in tracked)]


def working_copy():
    """The working tree's SDLC.md text, found from the repository root, or
    None when the file is missing."""
    root = git("rev-parse", "--show-toplevel").strip()
    path = os.path.join(root, SDLC)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv):
    args = argv[1:]
    try:
        if args == ["--check-paths"]:
            text = working_copy()
            listed = patterns(text) if text is not None else None
            if not listed:
                raise ValueError(f"{SDLC}: no independent_review.{KEY} list")
            root = git("rev-parse", "--show-toplevel").strip()
            tracked = [p for p in git("-C", root, "ls-files", "-z").split("\0") if p]
            missing = unmatched(listed, tracked)
            for p in missing:
                print(f"{SDLC}: cross_family_paths entry {p} matches no tracked file")
            return 1 if missing else 0
        sources = []
        if args[:1] == ["--base"] and len(args) == 2:
            base = args[1]
            root = git("rev-parse", "--show-toplevel").strip()
            try:
                sources.append(git("show", f"{base}:{SDLC}"))
            except subprocess.CalledProcessError:
                pass  # the base has no SDLC.md: only the working copy counts
            os.chdir(root)  # changed_files lists paths from the root
            paths = changed_files(base)
            if not paths:
                print(f"review_tier: no files differ from {base}", file=sys.stderr)
        elif args and not args[0].startswith("-"):
            paths = args
        else:
            print(__doc__.strip().split("\n\n")[2], file=sys.stderr)
            return 2
        text = working_copy()
        if text is not None:
            sources.append(text)
        sets = [s for s in (patterns(t) for t in sources) if s]
        if not sets:
            raise ValueError(f"no {SDLC} copy has independent_review.{KEY}; "
                             "refusing to pick a tier")
    except (subprocess.CalledProcessError, ValueError, OSError) as e:
        print(f"review_tier: {e}", file=sys.stderr)
        return 2
    hits = matches(sets, paths)
    if hits:
        print("cross-family")
        for p in hits:
            print(f"  {p}")
    else:
        print("agent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
