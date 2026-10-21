#!/usr/bin/env python3
"""Flag issue-closing keywords used in a negated or partial sentence (issue #674).

    python3 scripts/pr_body_lint.py BODY.md          # a PR body in a file
    python3 scripts/pr_body_lint.py -                # ... on stdin
    python3 scripts/pr_body_lint.py --env PR_BODY    # ... in an environment variable (CI)
    python3 scripts/pr_body_lint.py --git-range origin/main..HEAD   # commit messages too

GitHub closes an issue when a merged PR's body (or a commit message that lands on
the default branch) has a closing keyword right before an issue reference:
`close`, `closes`, `closed`, `fix`, `fixes`, `fixed`, `resolve`, `resolves`,
`resolved`, then `#N`, `owner/repo#N` or an issue URL, with an optional colon.
It reads the keyword, not the sentence, so "this does not fix #12" and "partially
fixes #12" close #12 on merge. This script fails on exactly that: a closing
keyword whose sentence also says it is negated or partial. Write `Refs #12` there.

HTML comments are ignored (GitHub does not read them). Code is not: whether
GitHub honours a keyword inside backticks is not something to rely on.

Exit status: 0 clean, 1 at least one flagged sentence, 2 a usage error.
"""
import argparse
import os
import re
import subprocess
import sys

# GitHub's closing keywords, then a reference: #N, owner/repo#N, or an issue URL.
CLOSE_RE = re.compile(
    r"\b(close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b:?\s+"
    r"((?:[\w.-]+/[\w.-]+)?#\d+|https://github\.com/[\w.-]+/[\w.-]+/issues/\d+)",
    re.IGNORECASE,
)

# Negation or partiality anywhere earlier in the same sentence: "does not fix",
# "doesn't fully fix", "partially fixes", "this never closed".
BEFORE_ANY = re.compile(
    r"\b(?:not|never|no longer|cannot|partially|partly)\b|n't\b",
    re.IGNORECASE,
)

# Partial wording only in the few words right before the keyword ("helps fix",
# "a first step towards fixing" is no keyword): such words elsewhere in a sentence
# ("adds help text and fixes #12") say nothing about the reference.
BEFORE_NEAR = re.compile(
    r"\b(?:helps?|help(?:s|ed)? to|towards?|begins? to|starts? to|almost|nearly|half|"
    r"part of|some of|without|in part)\s+(?:\w+\s+)?$",
    re.IGNORECASE,
)

# Words right after the reference that make it partial: "fixes #12 partially".
AFTER_CUES = re.compile(
    r"^\W*(?:only\s+)?(?:partially|partly|in part|for now|halfway|"
    r"(?:the\s+)?(?:first|second|third|next|last)\s+(?:half|part|step|slice)|"
    r"part\s+\d+|step\s+\d+)\b",
    re.IGNORECASE,
)

SENTENCE_SPLIT = re.compile(r"(?<=[.;!?])\s+|\n")


def strip_comments(text):
    """Blank out HTML comments, keeping line numbers."""
    return re.sub(r"<!--.*?-->", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)


def findings(text, source="body"):
    """(source, line number, sentence, why) for every negated or partial closer."""
    out = []
    text = strip_comments(text)
    for lineno, line in enumerate(text.splitlines(), 1):
        for sentence in SENTENCE_SPLIT.split(line):
            for m in CLOSE_RE.finditer(sentence):
                before = sentence[: m.start()]
                after = sentence[m.end() :]
                cue = BEFORE_ANY.search(before) or BEFORE_NEAR.search(before)
                if cue:
                    why = f"'{cue.group(0).strip()}' before '{m.group(0)}'"
                else:
                    cue = AFTER_CUES.search(after)
                    if not cue:
                        continue
                    why = f"'{cue.group(0).strip()}' after '{m.group(0)}'"
                out.append((source, lineno, sentence.strip(), why))
    return out


def commit_messages(rng):
    """The full messages of the commits in a git range, one string each."""
    r = subprocess.run(
        ["git", "log", "--format=%H%x00%B%x01", rng],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        raise SystemExit(f"pr_body_lint.py: git log {rng} failed: {r.stderr.strip()}")
    msgs = []
    for chunk in r.stdout.split("\x01"):
        chunk = chunk.strip("\n")
        if chunk:
            sha, _, body = chunk.partition("\x00")
            msgs.append((sha[:10], body))
    return msgs


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("file", nargs="?", help="PR body file, or - for stdin")
    ap.add_argument("--env", metavar="VAR", help="read the PR body from this environment variable")
    ap.add_argument("--git-range", metavar="A..B", help="also lint these commits' messages")
    args = ap.parse_args(argv)

    sources = []
    if args.env:
        sources.append(("PR body", os.environ.get(args.env, "")))
    if args.file == "-":
        sources.append(("PR body", sys.stdin.read()))
    elif args.file:
        try:
            with open(args.file, encoding="utf-8") as f:
                sources.append((args.file, f.read()))
        except OSError as e:
            print(f"pr_body_lint.py: {e}", file=sys.stderr)
            return 2
    if args.git_range:
        sources += [(f"commit {sha}", body) for sha, body in commit_messages(args.git_range)]
    if not sources:
        ap.print_usage(sys.stderr)
        return 2

    found = [f for name, text in sources for f in findings(text, name)]
    for source, lineno, sentence, why in found:
        print(f"{source}:{lineno}: closing keyword in a negated or partial sentence ({why}):")
        print(f"    {sentence}")
    if found:
        print(
            "GitHub closes the issue on merge whatever the sentence says. Write `Refs #N` for "
            "partial or related work, and `Closes #N` only on its own line for work that completes it."
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
