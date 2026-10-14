#!/bin/bash
# claim.sh <N> -- atomically claim GitHub issue <N> for a /work-issue run.
#
# Run it as ONE command, never abbreviated:
#   bash "$(git rev-parse --show-toplevel)/.claude/skills/work-issue/scripts/claim.sh" <N>
#
# The lock is the directory work-issue-lock-<N> in the main repo's git dir.
# Since #297 this script never moves or deletes it (only the step 9 release,
# `rm -rf`, removes it):
#   - a fresh claim is `mkdir LOCKDIR` plus a heartbeat file, exactly as before
#     (generation 0: the same bytes on disk as every older claim.sh writes);
#   - a stale reclaim is `mkdir LOCKDIR/gen.<K+1>`, where gen.<K> is the newest
#     generation it judged stale (K=0 when there is none). Generation dirs are
#     never deleted while the lock exists, so each one can be created once, by
#     one racer: the mkdir is the compare-and-swap.
# The heartbeat stays in LOCKDIR/heartbeat (one file shared by all generations),
# so heartbeat.sh and every older copy of it keep working unchanged.
#
# The older protocol moved the lock dir away to steal it, which left the lock
# NAME free for a moment; a late racer's fresh mkdir landed in that gap and two
# runs both claimed (#297). Nothing here frees the name. The reasoning, the read
# order, the old/new compatibility matrix and the residuals are in
# references/claim-protocol.md; scripts/test-claim.sh re-runs the race tests.
#
# Deliberately NOT `set -e` / `set -u`: the heartbeat read below is allowed to
# fail (a missing heartbeat is a handled case), and -e would kill the script
# silently with no RESULT= line.
#
# Prints exactly one RESULT= line on stdout: "claimed (...)", "live (...)" or
# "lost ...". Uses the caller's cwd to find the repo (git-common-dir).

N="$1"
case "$N" in
  ''|*[!0-9]*) echo "RESULT=error: usage: claim.sh <issue number>"; exit 1 ;;
esac

MAINGIT="$(git rev-parse --path-format=absolute --git-common-dir)"
[ -n "$MAINGIT" ] || { echo "RESULT=error: could not resolve the main .git dir"; exit 1; }
LOCKDIR="$MAINGIT/work-issue-lock-$N"
HB="$LOCKDIR/heartbeat"
# A per-process temp name: two writers sharing one temp file could rename the
# other's half-written file into place.
write_hb() { date +%s > "$HB.tmp.$$" && mv "$HB.tmp.$$" "$HB"; }
inode() { stat -f %i "$LOCKDIR" 2>/dev/null; }

claim() {
  if mkdir "$LOCKDIR" 2>/dev/null && write_hb; then
    RES="claimed (fresh)"
    return
  fi

  # Read order matters (claim-protocol.md "Read order"): container identity,
  # then the generation list, then that generation's age, then the heartbeat.
  # The mkdir below uses the K from this ONE listing, so anything another racer
  # creates after it collides with ours, however long this process stalls.
  I0=$(inode)
  if [ -z "$I0" ]; then
    RES="lost (the lock vanished while being read) — do not resume/claim; re-run claim.sh to re-check"
    return
  fi
  K=0
  for g in "$LOCKDIR"/gen.*; do
    k="${g##*/gen.}"
    case "$k" in ''|*[!0-9]*) continue ;; esac
    [ "$k" -gt "$K" ] && K="$k"
  done
  if [ "$K" = 0 ]; then NEWEST="$LOCKDIR"; else NEWEST="$LOCKDIR/gen.$K"; fi
  NOW=$(date +%s)
  # If the stat fails, the age is 0: fail toward live, never toward stale.
  M=$(stat -f %m "$NEWEST" 2>/dev/null); [ -n "$M" ] || M="$NOW"
  HB_CONTENT=$(cat "$HB" 2>/dev/null)

  # The newest generation under 10 s old is a claim in flight: its owner may not
  # have written the heartbeat yet, so the heartbeat we just read can be the
  # previous owner's stale one. (#35: an empty dir is what every claim looks
  # like between its mkdir and its heartbeat write.)
  if [ $(( NOW - M )) -lt 10 ]; then
    RES="live (a claim is in flight, under 10 s old) — do not resume/claim"
    return
  fi
  if printf '%s' "$HB_CONTENT" | grep -qE '^[0-9]+$'; then
    AGE=$(( NOW - HB_CONTENT ))
  else
    AGE=999999   # missing or garbage heartbeat on a lock past the grace window: dead
  fi
  if [ "$AGE" -lt 2700 ]; then
    RES="live (heartbeat $((AGE / 60)) min old) — do not resume/claim"
    return
  fi

  # Stale. Exactly one racer can create gen.<K+1>; a racer whose view is out of
  # date finds it taken and fails here.
  NEXT=$(( K + 1 ))
  if ! mkdir "$LOCKDIR/gen.$NEXT" 2>/dev/null; then
    RES="lost the reclaim race (generation $NEXT already taken) — do not resume/claim"
    return
  fi
  # We won generation NEXT of whatever container is at the name now. Confirm,
  # around the heartbeat write, that it is still the one we judged (a release
  # plus a fresh claim, or an older claim.sh that moves locks, changes the
  # inode) and that nobody reclaimed past us (only if we stalled over 10 s here).
  if [ "$(inode)" = "$I0" ] && write_hb && [ "$(inode)" = "$I0" ] \
     && [ -d "$LOCKDIR/gen.$NEXT" ] && [ ! -e "$LOCKDIR/gen.$(( NEXT + 1 ))" ]; then
    RES="claimed (reclaimed a stale lock)"
  else
    RES="lost the reclaim race (the lock changed after generation $NEXT was taken) — do not resume/claim"
  fi
}

claim
echo "RESULT=$RES"
