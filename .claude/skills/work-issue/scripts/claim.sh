#!/bin/bash
# claim.sh <N> -- atomically claim GitHub issue <N> for a /work-issue run.
#
# Run it as ONE command, never abbreviated:
#   bash "$(git rev-parse --show-toplevel)/.claude/skills/work-issue/scripts/claim.sh" <N>
#
# The body below the argument check is the claim script from
# references/claim-protocol.md, moved verbatim in #276 (round 6): the only
# change is that the issue number is "$N" instead of the placeholder <N>. The
# reasoning behind every guard is in claim-protocol.md; scripts/test-claim.sh
# re-runs the race tests and diffs this body against that original.
#
# Deliberately NOT `set -e` / `set -u`: the heartbeat read below is allowed to
# fail (a missing heartbeat is a handled case), and -e would kill the script
# silently with no RESULT= line.
#
# Prints exactly one RESULT= line: "claimed (...)", "live (...)" or "lost ...".
# Uses the caller's cwd to find the repo (git-common-dir), like the original.

N="$1"
case "$N" in
  ''|*[!0-9]*) echo "RESULT=error: usage: claim.sh <issue number>"; exit 1 ;;
esac

MAINGIT="$(git rev-parse --path-format=absolute --git-common-dir)"
[ -n "$MAINGIT" ] || { echo "RESULT=error: could not resolve the main .git dir"; exit 1; }
LOCKDIR="$MAINGIT/work-issue-lock-$N"

if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
  echo "RESULT=claimed (fresh)"
else
  HB="$LOCKDIR/heartbeat"
  ORIG_HB_CONTENT=$(cat "$HB" 2>/dev/null)
  if [ -n "$ORIG_HB_CONTENT" ] && printf '%s' "$ORIG_HB_CONTENT" | grep -qE '^[0-9]+$'; then
    AGE=$(( $(date +%s) - ORIG_HB_CONTENT ))
  else
    # Missing/unreadable heartbeat is ambiguous on its own: it's both a
    # genuinely crashed claim (died mid-truncation) AND what a *different*
    # concurrent invocation's own fresh mkdir looks like for the few
    # milliseconds between its mkdir succeeding and its heartbeat write
    # landing (verified empirically — treating this as unconditionally
    # stale let a second racer steal a lock the first racer had just won,
    # microseconds after winning it). Fall back to the lock dir's own
    # mtime: only a dir that's *also* been sitting untouched past a small
    # grace window is a genuine crash, not an in-flight claim.
    DIR_MTIME=$(stat -f %m "$LOCKDIR" 2>/dev/null || date +%s)
    DIR_AGE=$(( $(date +%s) - DIR_MTIME ))
    if [ "$DIR_AGE" -lt 10 ]; then
      AGE=0
    else
      AGE=999999
    fi
  fi

  if [ "$AGE" -lt 2700 ]; then
    echo "RESULT=live (heartbeat $((AGE / 60)) min old) — do not resume/claim"
  else
    STEAL="$LOCKDIR.stale.$$.$(date +%s)"
    if mv "$LOCKDIR" "$STEAL" 2>/dev/null; then
      MOVED_HB_CONTENT=$(cat "$STEAL/heartbeat" 2>/dev/null)
      if [ "$MOVED_HB_CONTENT" = "$ORIG_HB_CONTENT" ]; then
        if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
          rm -rf "$STEAL"
          echo "RESULT=claimed (reclaimed a stale lock)"
        else
          rm -rf "$STEAL"
          echo "RESULT=lost a rare 3-way reclaim race — do not resume/claim"
        fi
      else
        # ABA: the object we just moved isn't the stale one we decided
        # to steal -- a faster racer already reclaimed and recreated it
        # between our read and our mv, and we grabbed *their* fresh
        # claim by name instead (mv/rename has no concept of the
        # object's identity, only its path). Put it back and back off.
        mv "$STEAL" "$LOCKDIR" 2>/dev/null
        rm -rf "$STEAL" 2>/dev/null
        echo "RESULT=lost the reclaim race (ABA detected, restored) — do not resume/claim"
      fi
    else
      echo "RESULT=lost the reclaim race to a concurrent invocation — do not resume/claim"
    fi
  fi
fi
