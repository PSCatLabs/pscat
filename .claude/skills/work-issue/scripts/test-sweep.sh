#!/bin/bash
# test-sweep.sh -- check sweep-max.sh (#303). Run after any edit to it:
#   bash .claude/skills/work-issue/scripts/test-sweep.sh
# Uses a throwaway git repo and a fake hook; no pscat, no network.
HERE="$(cd "$(dirname "$0")" && pwd)"
SWEEP="$HERE/sweep-max.sh"
fail=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }
T="$(mktemp -d "${TMPDIR:-/tmp}/sweep-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
git -C "$T" init -q . && mkdir "$T/gallery" "$T/examples"
printf '%s\n' 'a 7' > "$T/gallery/a.ps"
printf '%s\n' 'b 12' > "$T/gallery/b.ps"
printf '%s\n' 'c 3.5' > "$T/examples/c.ps"
printf '%s\n' 'd 100' > "$T/examples/d.ps"
printf '%s\n' 'bad' > "$T/gallery/z.ps"
git -C "$T" add . && git -C "$T" -c user.email=t@t -c user.name=t commit -qm x
# The hook prints some noise, then the number in the file as its last line.
cat > "$T/hook.sh" <<'HOOK'
#!/bin/sh
[ "$#" -eq 1 ] || exit 9   # exactly one argument: the file
echo noise
n=$(sed -n 's/^[a-z] //p' "$1"); [ "$n" = bad ] && exit 3
echo "$n"
HOOK
chmod +x "$T/hook.sh"

out=$("$SWEEP" --tree "$T" --glob 'gallery/a.ps' --glob 'gallery/b.ps' --glob 'examples/*.ps' -j 3 -- "$T/hook.sh"); rc=$?
[ "$rc" -eq 0 ] || bad "clean sweep exited $rc"
[ "$(printf '%s\n' "$out" | tail -1)" = "MAX 100 examples/d.ps" ] || bad "wrong MAX: $(printf '%s\n' "$out" | tail -1)"
[ "$(printf '%s\n' "$out" | sed -n 2p)" = "12 gallery/b.ps" ] || bad "not sorted largest first: $out"
[ "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" = 5 ] || bad "expected 4 files + MAX: $out"

out=$("$SWEEP" --tree "$T" --glob 'gallery/*.ps' -- "$T/hook.sh"); rc=$?
[ "$rc" -eq 1 ] || bad "a failing hook must exit 1, got $rc"
printf '%s\n' "$out" | grep -qx 'FAIL gallery/z.ps' || bad "missing FAIL line: $out"
printf '%s\n' "$out" | grep -qx 'MAX 12 gallery/b.ps' || bad "FAIL file hid the real max: $out"

"$SWEEP" --tree "$T" --glob 'nothing/*.ps' -- "$T/hook.sh" >/dev/null 2>&1; [ $? -eq 64 ] || bad "empty match must exit 64"
"$SWEEP" --tree "$T" --glob 'gallery/*.ps' >/dev/null 2>&1; [ $? -eq 64 ] || bad "no hook must exit 64"
[ "$(git -C "$T" status --short)" = "?? hook.sh" ] || bad "the sweep changed the tree: $(git -C "$T" status --short)"

[ "$fail" -eq 0 ] && echo "test-sweep.sh: all passed"
exit "$fail"
