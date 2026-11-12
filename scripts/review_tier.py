#!/usr/bin/env python3
"""Which independent review does this PR need: cross-family (Codex) or agent?

SDLC.md's `independent_review.cross_family_paths` lists the paths where a bug
costs the most: engine and library code, the npm runtime, dependencies, the
launchers that run on users' machines and the release scripts. A PR that
touches one of them gets a cross-family review (Codex). Every other PR (docs,
skills, PostScript libraries, gallery, tests, CI) gets the same-family
blank-context agent review. The work-issue skill runs this script instead of
eyeballing globs:

    python3 scripts/review_tier.py --base origin/main   # PR vs main
    python3 scripts/review_tier.py path/a path/b        # explicit paths

It prints `cross-family` and then each matching path, indented, or `agent`.
Exit status 0 either way, 2 on a usage or git error.

The patterns use CODEOWNERS syntax (`scripts/codeowners_check.py`). They are
read from BOTH the base's SDLC.md and the working tree's copy, and a path
matching either one counts, so a PR cannot lower its own tier by editing the
list.
"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from codeowners_check import changed_files, git, to_regex  # noqa: E402

SDLC = "SDLC.md"
KEY = "cross_family_paths:"


def patterns(text):
    """The `independent_review.cross_family_paths` list from SDLC.md's text, or
    [] when it has none. The list is written on one line as a JSON array of
    strings, so no YAML parser is needed."""
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return []
    in_block = False
    for line in lines[1:]:
        if line.strip() == "---":
            break
        if not line.startswith(" "):
            in_block = line.startswith("independent_review:")
            continue
        stripped = line.strip()
        if in_block and stripped.startswith(KEY):
            value = json.loads(stripped[len(KEY):].strip())
            if not isinstance(value, list) or not all(isinstance(p, str) for p in value):
                raise ValueError(f"{SDLC}: {KEY} must be a list of strings")
            return value
    return []


def matches(pattern_sets, paths):
    """The paths that match any pattern in any set, in input order."""
    regexes = [to_regex(p) for ps in pattern_sets for p in ps]
    return [p for p in paths if any(r.match(p) for r in regexes)]


def main(argv):
    args = argv[1:]
    try:
        sources = []
        if args[:1] == ["--base"] and len(args) == 2:
            base = args[1]
            try:
                sources.append(git("show", f"{base}:{SDLC}"))
            except subprocess.CalledProcessError:
                pass  # the base has no SDLC.md: only the working copy counts
            paths = changed_files(base)
        elif args and not args[0].startswith("-"):
            paths = args
        else:
            print(__doc__.strip().split("\n\n")[2], file=sys.stderr)
            return 2
        if os.path.exists(SDLC):
            with open(SDLC, encoding="utf-8") as f:
                sources.append(f.read())
        sets = [patterns(s) for s in sources]
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
