#!/bin/bash
# heartbeat.sh <N> -- refresh the heartbeat of the lock claim.sh took for issue <N>.
#
#   bash "$(git rev-parse --show-toplevel)/.claude/skills/work-issue/scripts/heartbeat.sh" <N>
#
# The two commands below are the heartbeat refresh from references/claim-protocol.md
# ("Heartbeat writes after the claim"), moved verbatim in #276. It deliberately
# never runs `mkdir`: a missing lock dir means this run no longer owns the issue.
# If this fails with "No such file or directory": stop, never recreate the lock
# dir; re-run claim.sh and continue only if it says `claimed`.
# Not `set -e`: the status of the final command is the script's status.

N="$1"
case "$N" in
  ''|*[!0-9]*) echo "usage: heartbeat.sh <issue number>" >&2; exit 2 ;;
esac

LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-$N"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
