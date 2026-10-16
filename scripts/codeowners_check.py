#!/usr/bin/env python3
"""Which changed files fall under .github/CODEOWNERS? (issue #603)

SDLC.md's owner-merge rule says an agent never merges a PR that touches a
path listed in CODEOWNERS; the owner merges it. This script answers "does
this PR touch one?" so the work-issue skill does not have to eyeball glob
patterns:

    python3 scripts/codeowners_check.py --base origin/main   # PR vs main
    python3 scripts/codeowners_check.py path/a path/b        # explicit paths
    python3 scripts/codeowners_check.py --check-paths        # entries exist

`--base` lists the files changed between the merge base and HEAD (renames
count as both the old and the new name) and prints each one that matches a
CODEOWNERS rule, one per line. The rules are read from BOTH the base's copy
and the working tree's copy of the file, so a PR cannot get out of the rule by
deleting its own line from CODEOWNERS. Exit status 1 when anything matched
(the PR is owner-merged), 0 when nothing did, 2 on a usage or git error.

`--check-paths` is the "CODEOWNERS paths exist" check: every rule must match a
tracked file, unless its line ends in a `# forward: ...` comment, which marks
a path a planned issue will create (the rule is there first so protection is
in place before the file lands).

The pattern rules are the subset of gitignore syntax CODEOWNERS uses: a
leading `/` anchors to the repo root, a trailing `/` means a directory and
everything under it, a pattern with no `/` matches at any depth, `*` stops at
`/`, `**` does not, and the last matching rule wins (a rule with no owner
un-owns the path). `[...]` classes and `!` negation are not supported, and
GitHub does not support them in CODEOWNERS either.
"""
import re
import subprocess
import sys

CODEOWNERS = ".github/CODEOWNERS"


def parse(text):
    """[(pattern, owners, forward)] in file order. `forward` is True when the
    line carries a `# forward:` comment."""
    rules = []
    for raw in text.splitlines():
        body, _, comment = raw.partition("#")
        parts = body.split()
        if not parts:
            continue
        rules.append((parts[0], parts[1:], comment.strip().startswith("forward:")))
    return rules


def to_regex(pattern):
    """A CODEOWNERS pattern as a regex over a repo-relative POSIX path."""
    anchored = pattern.startswith("/") or "/" in pattern.rstrip("/")
    is_dir = pattern.endswith("/")
    core = pattern.strip("/")
    out = []
    i = 0
    while i < len(core):
        if core.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif core.startswith("**", i):
            out.append(".*")
            i += 2
        elif core[i] == "*":
            out.append("[^/]*")
            i += 1
        elif core[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(core[i]))
            i += 1
    prefix = "" if anchored else "(?:.*/)?"
    # A directory pattern owns everything under it; a bare name matches the
    # file or, if it is a directory, its contents (as gitignore does).
    tail = "/.*" if is_dir else "(?:/.*)?"
    return re.compile("^" + prefix + "".join(out) + tail + "$")


def owned(rules, path):
    """True when the last rule matching `path` names at least one owner."""
    verdict = False
    for pattern, owners, _ in rules:
        if to_regex(pattern).match(path):
            verdict = bool(owners)
    return verdict


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def changed_files(base):
    out = git("diff", "--name-only", "--no-renames", "-z", f"{base}...HEAD")
    # --no-renames lists a rename as a delete plus an add, so both names show.
    return [p for p in out.split("\0") if p]


def rules_from(text_sources):
    rules = []
    for text in text_sources:
        rules.extend(parse(text))
    return rules


def check_paths(rules, tracked):
    """The rules (as patterns) that match no tracked file and are not marked
    forward."""
    return [
        pattern
        for pattern, _, forward in rules
        if not forward and not any(to_regex(pattern).match(p) for p in tracked)
    ]


def main(argv):
    args = argv[1:]
    try:
        if args[:1] == ["--check-paths"]:
            with open(CODEOWNERS, encoding="utf-8") as f:
                rules = parse(f.read())
            tracked = [p for p in git("ls-files", "-z").split("\0") if p]
            missing = check_paths(rules, tracked)
            for pattern in missing:
                print(f"{CODEOWNERS}: {pattern} matches no tracked file "
                      "(add a `# forward: <issue>` comment if it is planned)")
            return 1 if missing else 0
        if args[:1] == ["--base"] and len(args) == 2:
            base = args[1]
            sources = []
            try:
                sources.append(git("show", f"{base}:{CODEOWNERS}"))
            except subprocess.CalledProcessError:
                pass  # the base has no CODEOWNERS yet
            try:
                with open(CODEOWNERS, encoding="utf-8") as f:
                    sources.append(f.read())
            except FileNotFoundError:
                pass
            rules = rules_from(sources)
            paths = changed_files(base)
        elif args and not args[0].startswith("-"):
            with open(CODEOWNERS, encoding="utf-8") as f:
                rules = parse(f.read())
            paths = args
        else:
            print(__doc__, file=sys.stderr)
            return 2
    except (subprocess.CalledProcessError, OSError) as e:
        print(f"codeowners_check: {e}", file=sys.stderr)
        return 2
    hits = [p for p in paths if owned(rules, p)]
    for p in hits:
        print(p)
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
