#!/usr/bin/env bash
# worktree_target.sh -- keep a git worktree's Cargo `target/` on external storage (#1281, #1033).
#
#   scripts/worktree_target.sh link   WORKTREE [NAME]
#   scripts/worktree_target.sh unlink WORKTREE
#
# PSCAT_TARGET_ROOT names a folder on another disk (the owner's ~/.zshrc exports it).
#   link    makes $PSCAT_TARGET_ROOT/NAME and symlinks WORKTREE/target to it. NAME defaults
#           to the worktree folder's name; work-issue passes `issue-<N>`. A no-op (exit 0)
#           when PSCAT_TARGET_ROOT is unset or not a folder, or when WORKTREE/target already
#           exists (file, folder or link), so it is safe to run twice.
#   unlink  deletes the folder WORKTREE/target links to, then the link. It acts only when
#           target is a symlink whose text is exactly $PSCAT_TARGET_ROOT/<one name>, directly
#           under the root (a trailing slash on the root is fine); anything else is left
#           alone and reported. A no-op when PSCAT_TARGET_ROOT is unset or target is no link.
#
# Two worktrees linked under one NAME share one folder, and `unlink` of either deletes it: give each
# a unique NAME (issue-<N>, idbase-<N>).
# Run `unlink` BEFORE `git worktree remove`. Every `git worktree add` in this repo, by hand or
# by a skill, is followed by `link`, and every removal by `unlink` (CLAUDE.md).
# Bash 3.2 compatible: no arrays, no readlink -f. Test: scripts/test_worktree_target.sh.
set -eu
die() { echo "worktree_target.sh: $*" >&2; exit 64; }
[ $# -ge 2 ] || die "usage: worktree_target.sh link WORKTREE [NAME] | unlink WORKTREE"
cmd=$1; wt=$2
[ -d "$wt" ] || die "'$wt' is not a folder"
root=${PSCAT_TARGET_ROOT:-}
while [ "${#root}" -gt 1 ] && [ "${root%/}" != "$root" ]; do root=${root%/}; done   # strip trailing slashes
[ -n "$root" ] && [ -d "$root" ] || exit 0
root=$(cd "$root" && pwd)   # absolute, so a link made from a relative root cannot dangle
# A name is one path component: no slash, not empty, not . or ..
ok_name() { case "$1" in ''|.|..|*/*) return 1 ;; esac; }
case "$cmd" in
  link)
    name=${3:-$(basename "$(cd "$wt" && pwd)")}
    ok_name "$name" || die "bad NAME '$name' (one folder name, no slash)"
    if [ -e "$wt/target" ] || [ -L "$wt/target" ]; then exit 0; fi
    mkdir -p "$root/$name"
    ln -s "$root/$name" "$wt/target"
    echo "worktree_target.sh: $wt/target -> $root/$name"
    ;;
  unlink)
    [ -L "$wt/target" ] || exit 0
    t=$(readlink "$wt/target")
    name=${t#"$root"/}
    if [ "$name" != "$t" ] && ok_name "$name"; then
      rm -rf -- "${root:?}/${name:?}"   # a symlink there is removed as a link, never followed
      rm -f -- "$wt/target"
      echo "worktree_target.sh: removed $root/$name"
    else
      echo "worktree_target.sh: leaving $wt/target alone: '$t' is not directly under $root" >&2
    fi
    ;;
  *) die "unknown command '$cmd' (link or unlink)" ;;
esac
