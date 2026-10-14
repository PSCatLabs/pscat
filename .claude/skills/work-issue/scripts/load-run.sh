#!/bin/bash
# load-run.sh -- run a command while N CPU hogs compete for the cores (#331).
#   bash .claude/skills/work-issue/scripts/load-run.sh [-n N] -- CMD [ARGS...]
# For the "does it still pass on a busy machine" check of a timing or concurrency
# change. N defaults to the core count. It records the PID of every hog it starts
# (printed to stderr) and on exit, ^C or SIGTERM kills ONLY those PIDs and the
# command it launched, never a pattern (SKILL.md: no pkill/killall). The command's
# exit status is the script's. Only the direct command is killed, so use `exec` or
# a single process, not a wrapper that forks. Bash 3.2 compatible.
N=""
while [ $# -gt 0 ]; do
    case "$1" in
        -n) [ $# -ge 2 ] || { echo "load-run.sh: -n needs a value" >&2; exit 64; }; N=$2; shift 2 ;;
        --) shift; break ;;
        *) echo "usage: load-run.sh [-n N] -- CMD [ARGS...]" >&2; exit 64 ;;
    esac
done
[ $# -ge 1 ] || { echo "usage: load-run.sh [-n N] -- CMD [ARGS...]" >&2; exit 64; }
[ -n "$N" ] || N=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4)
case "$N" in ''|*[!0-9]*|0) echo "load-run.sh: -n must be a positive whole number" >&2; exit 64 ;; esac
PIDS=()
cleanup() {
    trap '' EXIT INT TERM
    [ ${#PIDS[@]} -gt 0 ] && kill "${PIDS[@]}" 2>/dev/null
    [ ${#PIDS[@]} -gt 0 ] && wait "${PIDS[@]}" 2>/dev/null
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
for ((i = 0; i < N; i++)); do
    # A builtin-only loop: $! is the hog itself, with no child process to leak.
    ( while :; do :; done ) &
    PIDS+=($!)
    echo "load-run: hog pid $!" >&2
done
"$@" &
CMD=$!
PIDS+=($CMD)   # killed on an early exit too
wait "$CMD"
rc=$?
PIDS=("${PIDS[@]:0:$N}")   # the command is done: only hogs are left to kill
exit "$rc"
