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

HTML comments and code (fenced blocks, inline backticks) are ignored: GitHub
does not act on a reference there. A negation counts only right before the
keyword or right after the reference, so a negation about something else in the
same sentence ("no API change, and fixes #12") passes.

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

# The cue must sit right before the keyword, with at most two softening words
# between ("does not fix", "doesn't fully fix", "partially fixes", "helps fix").
# A negation further back belongs to another verb: "draws arcs that no longer
# overflow and fixes #12" closes #12 correctly and must pass.
FILLER = r"(?:(?:fully|completely|entirely|yet|really|quite|actually|totally|all)\s+){0,2}"
BEFORE = re.compile(
    r"(?:\b(?:not|never|cannot|no longer|partially|partly|helps?|towards?|almost|nearly|half|in part"
    r"|part of|some of|(?:begins?|starts?|helped|helps?) to)|n't)\s+" + FILLER + r"$",
    re.IGNORECASE,
)

# Right after the reference: "fixes #12 partially", "fixes #12 (not fully)",
# "fix #12 isn't complete", "fixes #12's first half". Only at the start of what
# follows, so "fixes #12 (curveto is not closed)" (the bug, not the fix) passes.
AFTER = re.compile(
    r"^(?:'s)?\W*(?:only\s+)?(?:partially|partly|in part|for now|halfway|"
    r"(?:the\s+)?(?:first|second|third|next|last)\s+(?:half|part|step|slice)|"
    r"(?:part|step)\s+(?:\d+|one|two|three|four)|"
    r"(?:is\s+)?not\s+(?:yet\s+)?(?:fully|completely|entirely|yet|all|done|complete|finished)|"
    r"isn't\s+(?:yet\s+)?(?:fully\s+)?(?:done|complete|finished|fixed)|only\s+(?:some|part))\b",
    re.IGNORECASE,
)

SENTENCE_SPLIT = re.compile(r"(?<=[.;!?])\s+|\n")


def _blank(m):
    return re.sub(r"[^\n]", " ", m.group(0))


def strip_hidden(text):
    """Blank what GitHub neither shows as prose nor reads for closing keywords:
    HTML comments (an unterminated one hides the rest of the body), fenced code
    blocks and inline code. Line numbers are kept."""
    text = re.sub(r"<!--.*?(?:-->|\Z)", _blank, text, flags=re.S)
    text = re.sub(r"^(```|~~~).*?(?:^\1[^\n]*$|\Z)", _blank, text, flags=re.S | re.M)
    return re.sub(r"`[^`\n]*`", _blank, text)


# A line that starts a new block (list item, heading, quote, table row) never
# continues the sentence of the line above it.
BLOCK_START = re.compile(r"^\s*(?:[-*+>|#]|\d+[.)])\s")


def carried_tail(prev):
    """The end of the previous line, if the sentence runs on into the next line.

    Hard-wrapped text (commit messages, wrapped Markdown) splits "does not" and
    "fix #12" over two lines, so the last few words of an unfinished line are
    checked with the next line's start.
    """
    stripped = prev.rstrip()
    if not stripped or stripped[-1] in ".;:!?":
        return ""
    if re.match(r"^\s*(?:#|\||<)", stripped):  # a heading, table row or HTML line ends where it is
        return ""
    tail = SENTENCE_SPLIT.split(stripped)[-1]
    return " ".join(tail.split()[-3:]) + " "


def findings(text, source="body"):
    """(source, line number, sentence, why) for every negated or partial closer."""
    out = []
    # Curly apostrophes ("doesn\u2019t") read as straight ones.
    text = strip_hidden(text).replace("\u2019", "'")
    prev = ""
    for lineno, line in enumerate(text.splitlines(), 1):
        # A list item's text continues only on an indented line below it.
        if BLOCK_START.match(line) or (BLOCK_START.match(prev) and not line[:1].isspace()):
            carry = ""
        else:
            carry = carried_tail(prev)
        prev = line
        for k, sentence in enumerate(SENTENCE_SPLIT.split(line)):
            skip = 0
            if k == 0:
                sentence, skip = carry + sentence, len(carry)
            for m in CLOSE_RE.finditer(sentence):
                if m.start() < skip:  # the previous line's own closer: already checked there
                    continue
                cue = BEFORE.search(sentence[: m.start()])
                if cue:
                    why = f"'{cue.group(0).strip()}' before '{m.group(0)}'"
                else:
                    cue = AFTER.search(sentence[m.end() :])
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
