#!/usr/bin/env bash
# sweep-max.sh -- "the max value fed to X across all pieces" sweep (#303).
#
#   sweep-max.sh [-j N] [--tree DIR] [--glob PATTERN]... -- HOOK [ARG...]
#
# Runs HOOK once per tracked file, in parallel, as `HOOK ARG... FILE` with DIR as the
# working directory (default: the repo this script lives in). The hook prints a number
# as the LAST line of its stdout (a probe that runs the piece and reports, say, the
# largest argument it passed to the thing under test). The script prints one line per
# file, `<value> <file>`, largest first, then `MAX <value> <file>`.
#
#   --glob PATTERN  which tracked files (git ls-files pathspec, quote it).
#                   Repeatable. Default: 'gallery/*.ps' 'examples/*.ps'.
#   -j N            parallel hooks (default: CPU count)
#
# A hook that exits nonzero or ends without a number is reported as `FAIL <file>`; the
# script then exits 1 even if other files gave numbers. Exit 64 is a usage error.
# A sweep over the gallery takes minutes: run it with `run_in_background: true` and
# a log file (pitfalls.md "Long foreground commands"). Bash 3.2 compatible.
#
# Example (largest page count any piece draws):
#   sweep-max.sh --glob 'gallery/*.ps' -- ./probe-pages.sh
set -euo pipefail
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
die() { echo "sweep-max.sh: $*" >&2; exit 64; }

if [ "${1:-}" = "--one" ]; then   # internal: --one TREE HOOK [ARG...] FILE
    tree=$2; shift 2
    file=""; for file; do :; done   # the last argument
    out=$( (cd "$tree" && "$@") 2>/dev/null) && rc=0 || rc=$?
    last=$(printf '%s\n' "$out" | tail -n 1)
    case "$last" in
        ''|*[!0-9eE.+-]*|[eE]*) rc=1 ;;
    esac
    if [ "$rc" -eq 0 ]; then printf 'OK %s %s\n' "$last" "$file"; else printf 'FAIL %s\n' "$file"; fi
    exit 0
fi

TREE=""; JOBS=""; GLOBS=""
while [ $# -gt 0 ]; do
    case "$1" in
        --tree) [ $# -ge 2 ] || die "--tree needs a directory"; TREE=$2; shift 2 ;;
        --glob) [ $# -ge 2 ] || die "--glob needs a pattern"; GLOBS="${GLOBS:+$GLOBS
}$2"; shift 2 ;;
        -j)     [ $# -ge 2 ] || die "-j needs a number"; JOBS=$2; shift 2 ;;
        -h|--help) sed -n '2,/^set -euo/p' "$SELF" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
        --)     shift; break ;;
        *)      die "unknown argument $1 (see --help; the hook goes after --)" ;;
    esac
done
[ $# -ge 1 ] || die "usage: sweep-max.sh [-j N] [--glob PATTERN]... -- HOOK [ARG...]"
[ -n "$TREE" ] || TREE="$(cd "$(dirname "$SELF")/../../../.." && pwd -P)"
[ -d "$TREE" ] || die "--tree '$TREE' is not a directory"
[ -n "$GLOBS" ] || GLOBS="gallery/*.ps
examples/*.ps"
[ -n "$JOBS" ] || JOBS=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4)
case "$JOBS" in ''|*[!0-9]*|0) die "-j takes a positive number" ;; esac
# Make a relative hook path absolute (the hook runs with TREE as its cwd).
HOOK=$1; shift
case "$HOOK" in */*) HOOK="$(cd "$(dirname "$HOOK")" && pwd -P)/$(basename "$HOOK")" ;; esac
[ -x "$HOOK" ] || command -v "$HOOK" >/dev/null 2>&1 || die "hook '$HOOK' is not executable"

LIST=$(mktemp "${TMPDIR:-/tmp}/sweep-max.XXXXXX")
RES=$(mktemp "${TMPDIR:-/tmp}/sweep-max.XXXXXX")
trap 'rm -f "$LIST" "$RES"' EXIT
set -f   # patterns are for git, not the shell
oldifs=$IFS; IFS='
'
for g in $GLOBS; do git -C "$TREE" ls-files -- "$g"; done | awk '!s[$0]++' > "$LIST"
IFS=$oldifs; set +f
[ -s "$LIST" ] || die "no tracked file matches the --glob patterns in $TREE"

tr '\n' '\0' < "$LIST" | xargs -0 -n 1 -P "$JOBS" "$SELF" --one "$TREE" "$HOOK" "$@" > "$RES"

bad=$(grep -c '^FAIL ' "$RES" || true)
grep '^OK ' "$RES" | sed 's/^OK //' | sort -g -r -k1,1 || true
grep '^FAIL ' "$RES" || true
top=$(grep '^OK ' "$RES" | sed 's/^OK //' | sort -g -r -k1,1 | head -n 1 || true)
if [ -n "$top" ]; then echo "MAX $top"; else echo "MAX none"; fi
[ "$bad" -eq 0 ] || exit 1
