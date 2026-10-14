#!/bin/bash
# claim.sh <N> -- atomically claim GitHub issue <N> for a /work-issue run.
#
# Run it as ONE command, never abbreviated:
#   bash "$(git rev-parse --show-toplevel)/.claude/skills/work-issue/scripts/claim.sh" <N>
#
# attempt() below is the claim block from references/claim-protocol.md (moved
# here in #276), with the issue number as "$N". #281 added two things:
#   - an inode identity check on the stale steal: before the mv (so a racer
#     whose decision is out of date does not move a live object at all) and
#     after it (alongside the heartbeat-content check, which cannot tell two
#     empty lock dirs apart);
#   - a bounded retry for the one racer that moved the stale lock away but then
#     lost its own re-create ("3-way" below). Before #281 that racer reported
#     `lost` and the race could end with zero winners. It now waits for the
#     lock to settle (a heartbeat appears, or the leftover dir passes the 10 s
#     grace window) and runs attempt() again, up to 3 attempts in all.
# The reasoning behind every guard is in claim-protocol.md; scripts/test-claim.sh
# re-runs the race tests. Worst-case runtime is about 30 s (two settle waits).
#
# Deliberately NOT `set -e` / `set -u`: the heartbeat read below is allowed to
# fail (a missing heartbeat is a handled case), and -e would kill the script
# silently with no RESULT= line.
#
# Prints exactly one RESULT= line on stdout: "claimed (...)", "live (...)" or
# "lost ...", with " [attempt K]" appended after a retry. Earlier attempts are
# logged on stderr as "claim.sh: attempt K: ..." (never starting with RESULT=).
# Uses the caller's cwd to find the repo (git-common-dir), like the original.

N="$1"
case "$N" in
  ''|*[!0-9]*) echo "RESULT=error: usage: claim.sh <issue number>"; exit 1 ;;
esac

MAINGIT="$(git rev-parse --path-format=absolute --git-common-dir)"
[ -n "$MAINGIT" ] || { echo "RESULT=error: could not resolve the main .git dir"; exit 1; }
LOCKDIR="$MAINGIT/work-issue-lock-$N"

# attempt: one pass of the claim protocol. Sets RES (the RESULT text) and RETRY
# (1 only for the 3-way loss, the one outcome that can leave the lock unowned).
attempt() {
  RETRY=0
  if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
    RES="claimed (fresh)"
    return
  fi
  HB="$LOCKDIR/heartbeat"
  # Identity and age of the object we are about to judge, from ONE stat, taken
  # before the heartbeat read (#281). An empty ORIG_INODE (stat failed) must
  # never reach the mv below.
  ORIG_STAT=$(stat -f '%i %m' "$LOCKDIR" 2>/dev/null)
  ORIG_INODE="${ORIG_STAT%% *}"
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
    # grace window is a genuine crash, not an in-flight claim. If stat
    # failed, the mtime is "now": fail toward live, never toward stale.
    DIR_MTIME="${ORIG_STAT##* }"
    [ -n "$ORIG_STAT" ] || DIR_MTIME=$(date +%s)
    DIR_AGE=$(( $(date +%s) - DIR_MTIME ))
    if [ "$DIR_AGE" -lt 10 ]; then
      AGE=0
    else
      AGE=999999
    fi
  fi

  if [ "$AGE" -lt 2700 ]; then
    RES="live (heartbeat $((AGE / 60)) min old) — do not resume/claim"
    return
  fi
  STEAL="$LOCKDIR.stale.$$.$ATTEMPT.$(date +%s)"
  if [ -z "$ORIG_INODE" ] || [ "$(stat -f %i "$LOCKDIR" 2>/dev/null)" != "$ORIG_INODE" ]; then
    # Pre-check (#281): the object at the lock name is no longer the one we
    # judged stale, so a faster racer already reclaimed it. Moving it anyway
    # would be an ABA move, and every ABA move frees the lock name for a
    # moment (see claim-protocol.md). Back off without touching it.
    RES="lost the reclaim race (lock replaced before the steal) — do not resume/claim"
  elif mv "$LOCKDIR" "$STEAL" 2>/dev/null; then
    MOVED_INODE=$(stat -f %i "$STEAL" 2>/dev/null)
    MOVED_HB_CONTENT=$(cat "$STEAL/heartbeat" 2>/dev/null)
    if [ "$MOVED_INODE" = "$ORIG_INODE" ] && [ "$MOVED_HB_CONTENT" = "$ORIG_HB_CONTENT" ]; then
      if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
        rm -rf "$STEAL"
        RES="claimed (reclaimed a stale lock)"
      else
        rm -rf "$STEAL"
        RES="lost a rare 3-way reclaim race — do not resume/claim"
        RETRY=1
      fi
    else
      # ABA: the object we just moved isn't the stale one we decided
      # to steal -- a faster racer already reclaimed and recreated it
      # between our read and our mv, and we grabbed *their* fresh
      # claim by name instead (mv/rename has no concept of the
      # object's identity, only its path). The inode says so even when
      # both heartbeats are empty. Put it back and back off.
      mv "$STEAL" "$LOCKDIR" 2>/dev/null
      rm -rf "$STEAL" 2>/dev/null
      RES="lost the reclaim race (ABA detected, restored) — do not resume/claim"
    fi
  else
    RES="lost the reclaim race to a concurrent invocation — do not resume/claim"
  fi
}

# settle: wait (at most ~13 s) until the lock has a numeric heartbeat (someone
# owns it) or the dir has existed past the 10 s grace window (an orphan the
# next attempt may reclaim). A MISSING dir does not end the wait: the 3-way
# loss happens while an ABA mover has the dir moved away, and a mkdir into that
# gap is exactly the transit-window double claim (#297).
settle() {
  local deadline=$(( $(date +%s) + 13 )) hb m
  while [ "$(date +%s)" -lt "$deadline" ]; do
    sleep 0.25
    hb=$(cat "$LOCKDIR/heartbeat" 2>/dev/null)
    if [ -n "$hb" ] && printf '%s' "$hb" | grep -qE '^[0-9]+$'; then return; fi
    m=$(stat -f %m "$LOCKDIR" 2>/dev/null) && [ $(( $(date +%s) - m )) -ge 10 ] && return
  done
}

for ATTEMPT in 1 2 3; do
  attempt
  [ "$RETRY" = 1 ] && [ "$ATTEMPT" -lt 3 ] || break
  echo "claim.sh: attempt $ATTEMPT: $RES; re-checking once the lock settles" >&2
  settle
done
if [ "$ATTEMPT" -gt 1 ]; then RES="$RES [attempt $ATTEMPT]"; fi
echo "RESULT=$RES"
