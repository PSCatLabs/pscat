#!/usr/bin/env bash
# bench_compare.sh -- interleaved best-of-N timing of two pscat binaries on one
# file, with an identical-binary control (issue #335). A guard, not a goal: use
# it to show a src/ change did not slow a render, not to tune.
#
#   scripts/bench_compare.sh BASE_BIN HEAD_BIN FILE [N] [-- extra pscat args]
#
# Per round it runs base, head, then base again as the CONTROL (the same binary
# twice, so the control-vs-base gap is the noise floor). Rounds interleave so
# load drift hits all three alike. The time is CPU (user+sys) seconds of the
# child, not wall time, so other agents on the machine do not move it.
# Prints min and median per arm; a head/base gap well inside the control's gap
# is noise. Run from the tree that FILE's relative paths resolve in. Each round
# prints its three times to stderr as it finishes, so a run you stop early (the
# coordinator narrowed the scope) still leaves its partial sample in the log.
# Needs perl. Bash 3.2 compatible.
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: bench_compare.sh BASE_BIN HEAD_BIN FILE [N] [-- pscat args]" >&2; exit 64; }
BASE=$1; HEAD=$2; FILE=$3; shift 3
N=7; case "${1:-}" in ''|--) ;; *) N=$1; shift ;; esac
case "$N" in ''|*[!0-9]*|0) echo "bench_compare.sh: N must be a positive whole number" >&2; exit 64 ;; esac
[ "${1:-}" = "--" ] && shift
cpu() { # BIN: CPU seconds of one headless render (pscat's own output is discarded)
    perl -e 'open(my $o, ">&", \*STDOUT); open(STDOUT, ">", "/dev/null"); open(STDERR, ">", "/dev/null");
             system(@ARGV) == 0 or exit 1; my @t = times; printf $o "%.3f\n", $t[2] + $t[3]' \
        "$1" --headless "$FILE" "${@:2}" || { echo "bench_compare.sh: $1 failed on $FILE" >&2; exit 1; }
}
B=(); H=(); C=()
for ((i = 0; i < N; i++)); do
    B+=("$(cpu "$BASE" "$@")"); H+=("$(cpu "$HEAD" "$@")"); C+=("$(cpu "$BASE" "$@")")
    echo "round $((i + 1)): base ${B[$i]} head ${H[$i]} control ${C[$i]}" >&2
done
stat() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END {printf "min %.3f  median %.3f", a[1], (NR%2 ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2)}'; }
echo "base     $(stat "${B[@]}")"
echo "head     $(stat "${H[@]}")"
echo "control  $(stat "${C[@]}")   (base binary again: the noise floor)"
