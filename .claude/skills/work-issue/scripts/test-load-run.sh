#!/bin/bash
# test-load-run.sh -- check load-run.sh (#331). Run after any edit to it:
#   bash .claude/skills/work-issue/scripts/test-load-run.sh
HERE="$(cd "$(dirname "$0")" && pwd)"
LR="$HERE/load-run.sh"
fail=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }
T="$(mktemp -d "${TMPDIR:-/tmp}/load-run-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
alive() { ps -p "$1" >/dev/null 2>&1; }

# 1. Normal exit: the command's status comes back and every hog is gone.
bash "$LR" -n 3 -- sh -c 'exit 7' 2> "$T/err1"; rc=$?
[ "$rc" -eq 7 ] || bad "exit status not propagated: $rc"
[ "$(grep -c 'hog pid' "$T/err1")" = 3 ] || bad "expected 3 hogs: $(cat "$T/err1")"
for p in $(sed -n 's/.*hog pid //p' "$T/err1"); do alive "$p" && bad "hog $p survived a normal exit"; done

# 2. SIGTERM to the script mid-run: hogs and the command are gone, status 143.
bash "$LR" -n 2 -- sh -c 'echo cmd $$ > '"$T"'/cmdpid; exec sleep 30' 2> "$T/err2" &
S=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$T/cmdpid" ] && break; python3 -c 'import time; time.sleep(0.3)'; done
kill -TERM "$S"; wait "$S"; rc=$?
[ "$rc" -eq 143 ] || bad "SIGTERM exit status $rc, want 143"
python3 -c 'import time; time.sleep(0.5)'
for p in $(sed -n 's/.*hog pid //p' "$T/err2") $(sed -n 's/^cmd //p' "$T/cmdpid"); do alive "$p" && bad "pid $p survived SIGTERM"; done

# 3. Bad arguments.
bash "$LR" -n 0 -- true 2>/dev/null; [ $? -eq 64 ] || bad "-n 0 must exit 64"
bash "$LR" 2>/dev/null; [ $? -eq 64 ] || bad "no command must exit 64"
[ "$fail" -eq 0 ] && echo "test-load-run: ok" || { echo "test-load-run: $fail failure(s)"; exit 1; }
