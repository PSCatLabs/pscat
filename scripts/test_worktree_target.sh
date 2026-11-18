#!/bin/bash
# Tests for scripts/worktree_target.sh (#1281). Run: /bin/bash scripts/test_worktree_target.sh
# Every case uses its own temporary PSCAT_TARGET_ROOT (or unsets it); the real one is never touched.
HERE="$(cd "$(dirname "$0")" && pwd)"
H="$HERE/worktree_target.sh"
fail=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }
REAL_ROOT="${PSCAT_TARGET_ROOT:-}"
REAL_LIST=""; [ -n "$REAL_ROOT" ] && [ -d "$REAL_ROOT" ] && REAL_LIST="$(ls -A "$REAL_ROOT")"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/wt-target-test.XXXXXX")" || exit 1
trap 'rm -rf "$TMP"' EXIT
TMP="$(cd "$TMP" && pwd)"   # TMPDIR may end in a slash; the helper normalizes the root
n=0
fresh() { n=$((n + 1)); ROOT="$TMP/root$n"; WT="$TMP/wt$n"; mkdir -p "$ROOT" "$WT"; }
run() { PSCAT_TARGET_ROOT="$ROOT" bash "$H" "$@" >/dev/null 2>&1; }

# link: makes the folder and the symlink
fresh; run link "$WT" issue-7
[ "$(readlink "$WT/target")" = "$ROOT/issue-7" ] && [ -d "$ROOT/issue-7" ] || bad "link did not create target -> root/issue-7"
# link: default name is the worktree folder's name
fresh; run link "$WT"
[ -d "$ROOT/wt$n" ] && [ -L "$WT/target" ] || bad "link default name"
# link: twice is fine, and a second name does not retarget an existing link
fresh; run link "$WT" a; run link "$WT" b
[ "$(readlink "$WT/target")" = "$ROOT/a" ] && [ ! -e "$ROOT/b" ] || bad "second link must be a no-op"
# link: an existing real target/ folder is left alone
fresh; mkdir "$WT/target"; run link "$WT" a
[ ! -L "$WT/target" ] && [ ! -e "$ROOT/a" ] || bad "link over an existing target folder"
# link: a dangling symlink counts as existing
fresh; ln -s /nonexistent "$WT/target"; run link "$WT" a
[ "$(readlink "$WT/target")" = /nonexistent ] || bad "link over a dangling link"
# link: unset or missing root is a no-op, exit 0
fresh; (unset PSCAT_TARGET_ROOT; bash "$H" link "$WT" a) >/dev/null 2>&1 || bad "unset root must exit 0"
[ ! -e "$WT/target" ] || bad "unset root made a link"
fresh; PSCAT_TARGET_ROOT="$TMP/nope" bash "$H" link "$WT" a >/dev/null 2>&1 || bad "missing root must exit 0"
[ ! -e "$WT/target" ] && [ ! -e "$TMP/nope" ] || bad "missing root made something"
fresh; PSCAT_TARGET_ROOT="" bash "$H" link "$WT" a >/dev/null 2>&1 || bad "empty root must exit 0"
# link: bad names are refused
for bad_name in .. . "a/b" "../x"; do
  fresh; run link "$WT" "$bad_name" && bad "name '$bad_name' accepted"
  [ ! -e "$WT/target" ] || bad "name '$bad_name' made a link"
done

# unlink: removes the folder and the link
fresh; run link "$WT" issue-7; echo x > "$ROOT/issue-7/f"; run unlink "$WT"
[ ! -e "$ROOT/issue-7" ] && [ ! -L "$WT/target" ] || bad "unlink did not remove folder and link"
# unlink: trailing slash on the root still matches
fresh; run link "$WT" issue-7; PSCAT_TARGET_ROOT="$ROOT/" bash "$H" unlink "$WT" >/dev/null 2>&1
[ ! -e "$ROOT/issue-7" ] || bad "unlink with a trailing-slash root"
# unlink: link text with a trailing slash or a double slash is not the exact path: left alone
fresh; mkdir "$ROOT/x"; ln -s "$ROOT/x/" "$WT/target"; run unlink "$WT"
[ -d "$ROOT/x" ] && [ -L "$WT/target" ] || bad "unlink must leave 'root/x/' alone"
# unlink: a link outside the root is left alone
fresh; mkdir "$TMP/outside$n"; ln -s "$TMP/outside$n" "$WT/target"; run unlink "$WT"
[ -d "$TMP/outside$n" ] && [ -L "$WT/target" ] || bad "unlink deleted a folder outside the root"
# unlink: nested path under the root is left alone
fresh; mkdir -p "$ROOT/a/b"; ln -s "$ROOT/a/b" "$WT/target"; run unlink "$WT"
[ -d "$ROOT/a/b" ] || bad "unlink deleted a nested folder"
# unlink: the root itself, and a .. escape, are left alone
fresh; ln -s "$ROOT" "$WT/target"; run unlink "$WT"
[ -d "$ROOT" ] || bad "unlink deleted the root"
fresh; mkdir "$ROOT/x" "$TMP/victim$n"; ln -s "$ROOT/../victim$n" "$WT/target"; run unlink "$WT"
[ -d "$TMP/victim$n" ] || bad "unlink followed .. out of the root"
fresh; ln -s "$ROOT/.." "$WT/target"; run unlink "$WT"
[ -d "$ROOT" ] && [ -d "$TMP" ] && [ -L "$WT/target" ] || bad "unlink of root/.. "
# unlink: a relative link is left alone
fresh; mkdir "$WT/rel"; ln -s rel "$WT/target"; run unlink "$WT"
[ -d "$WT/rel" ] || bad "unlink deleted a relative target"
# unlink: a symlink inside the root is removed as a link; its destination survives
fresh; mkdir "$TMP/dest$n"; ln -s "$TMP/dest$n" "$ROOT/lnk"; ln -s "$ROOT/lnk" "$WT/target"; run unlink "$WT"
[ -d "$TMP/dest$n" ] && [ ! -e "$ROOT/lnk" ] && [ ! -L "$ROOT/lnk" ] || bad "unlink followed a symlink in the root"
# unlink: a real target/ folder, a missing target, and an unset root are no-ops
fresh; mkdir "$WT/target"; echo x > "$WT/target/f"; run unlink "$WT"
[ -f "$WT/target/f" ] || bad "unlink removed a real target folder"
fresh; run unlink "$WT" || bad "unlink with no target must exit 0"
fresh; run link "$WT" a; (unset PSCAT_TARGET_ROOT; bash "$H" unlink "$WT") >/dev/null 2>&1
[ -d "$ROOT/a" ] && [ -L "$WT/target" ] || bad "unlink with an unset root must do nothing"

# a relative root is made absolute, so the link cannot dangle
fresh; (cd "$TMP" && PSCAT_TARGET_ROOT="root$n" bash "$H" link "$WT" a >/dev/null 2>&1)
[ "$(readlink "$WT/target")" = "$ROOT/a" ] && [ -d "$WT/target" ] || bad "relative root"
(cd "$TMP" && PSCAT_TARGET_ROOT="root$n" bash "$H" unlink "$WT" >/dev/null 2>&1)
[ ! -e "$ROOT/a" ] || bad "unlink with a relative root"

# usage errors
bash "$H" >/dev/null 2>&1 && bad "no arguments must fail"
bash "$H" link "$TMP/no-such-dir" >/dev/null 2>&1 && bad "missing worktree must fail"
fresh; run frob "$WT" && bad "unknown command must fail"

# the real root is untouched
if [ -n "$REAL_ROOT" ] && [ -d "$REAL_ROOT" ] && [ "$(ls -A "$REAL_ROOT")" != "$REAL_LIST" ]; then bad "the real PSCAT_TARGET_ROOT changed"; fi

if [ "$fail" -eq 0 ]; then echo "worktree_target tests: ok"; else echo "worktree_target tests: $fail failed"; exit 1; fi
