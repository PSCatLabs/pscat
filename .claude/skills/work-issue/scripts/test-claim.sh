#!/bin/bash
# test-claim.sh -- re-run the claim.sh race tests (#35, #72, #276). Run it after ANY
# edit to claim.sh:   bash .claude/skills/work-issue/scripts/test-claim.sh
#
# Everything happens in a throwaway git repo, so no real work-issue lock is touched.
# Racers wait on a start barrier (a `go` file) so they really overlap; each trial
# asserts exactly one winner. macOS (BSD stat/date/touch), like claim.sh.
#
# Optional: CLAIM_ORIG=<file> diffs claim.sh against the original claim block (the
# first ```sh block of claim-protocol.md before #276: `git show <ref>:<path>`), so
# "the commands are verbatim" is checked, not asserted.
#
# Counts: FRESH_TRIALS (default 30) and STALE_TRIALS (default 20) match the numbers
# the protocol documents; RACERS per trial defaults to 4.

HERE="$(cd "$(dirname "$0")" && pwd)"
CLAIM="$HERE/claim.sh"
FRESH_TRIALS="${FRESH_TRIALS:-30}"; STALE_TRIALS="${STALE_TRIALS:-20}"; RACERS="${RACERS:-4}"
T="$(mktemp -d "${TMPDIR:-/tmp}/claim-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
git init -q "$T/repo" || exit 1
LOCKS="$T/repo/.git"
fail=0; contended=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }

# race <issue> <script> [<script> ...]: launch every script at once, collect RESULT lines in $T/out.*
race() {
  local n="$1"; shift; rm -f "$T"/out.* "$T/go"; local i=0
  for s in "$@"; do
    i=$((i + 1))
    ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; bash "$s" "$n" > "$T/out.$i" 2>&1 ) &
  done
  sleep 0.2; touch "$T/go"; wait
}
count() { cat "$T"/out.* | grep -c "^RESULT=$1"; }
lock() { echo "$LOCKS/work-issue-lock-$1"; }
stale_hb() { mkdir "$(lock "$1")"; echo $(( $(date +%s) - ${2:-3000} )) > "$(lock "$1")/heartbeat"; }
backdate() { touch -t "$(date -v-60S +%Y%m%d%H%M.%S)" "$1"; }
many() { local s=(); local k; for k in $(seq 1 "$RACERS"); do s+=("${1:-$CLAIM}"); done; echo "${s[@]}"; }

if [ -n "$CLAIM_ORIG" ]; then
  if diff <(sed 's/<N>/$N/' "$CLAIM_ORIG") <(sed -n '/^MAINGIT=/,$p' "$CLAIM") >/dev/null; then
    echo "verbatim: claim.sh body == original block (with <N> -> \$N)"
  else bad "claim.sh body differs from CLAIM_ORIG"; fi
fi

# 1. fresh claims: one mkdir winner, the rest live (the empty-dir grace window) or lost
ok=0
for t in $(seq 1 "$FRESH_TRIALS"); do
  rm -rf "$(lock 100)"; race 100 $(many)
  c=$(count 'claimed (fresh)'); o=$(( $(count 'live') + $(count 'lost') ))
  if [ "$c" = 1 ] && [ $((c + o)) = "$RACERS" ]; then ok=$((ok + 1)); else bad "fresh trial $t: claimed=$c other=$o"; fi
done
echo "fresh races:           $ok/$FRESH_TRIALS trials with exactly 1 winner ($RACERS racers each)"

# 2. stale-heartbeat reclaims: AT MOST one winner (a double claim is the failure). Zero winners
#    is a safe outcome the protocol allows (a racer's ABA restore can hit another racer's
#    in-flight reclaim; everyone reports `lost`, the lock stays reclaimable): counted, not failed.
ok=0; lost=0; zero=0; reclaimed=0; fresh2=0
for t in $(seq 1 "$STALE_TRIALS"); do
  rm -rf "$LOCKS"/work-issue-lock-101*; stale_hb 101; race 101 $(many)
  c=$(count 'claimed'); l=$(count 'lost'); lv=$(count 'live')
  reclaimed=$((reclaimed + $(count 'claimed (reclaimed'))); fresh2=$((fresh2 + $(count 'claimed (fresh)')))
  lost=$((lost + l)); [ $((l + lv)) -gt 0 ] && contended=$((contended + 1)); [ "$c" = 0 ] && zero=$((zero + 1))
  if [ "$c" -le 1 ] && [ $((c + l + lv)) = "$RACERS" ]; then ok=$((ok + 1)); else bad "stale trial $t: claimed=$c lost=$l live=$lv :: $(cat "$T"/out.* | tr '\n' '|')"; fi
done
echo "stale-reclaim races:   $ok/$STALE_TRIALS trials with no double claim ($reclaimed reclaimed, $fresh2 fresh-after-steal, $zero with zero winners; $lost racers reported 'lost')"

# 3. empty lock dir (no heartbeat yet): under 10 s old it is an in-flight claim -> all live
ok=0
for t in $(seq 1 5); do
  rm -rf "$(lock 102)"; mkdir "$(lock 102)"; race 102 $(many)
  [ "$(count live)" = "$RACERS" ] && ok=$((ok + 1)) || bad "young empty dir trial $t not all live"
done
echo "young empty lock dir:  $ok/5 trials all live"
# ... and backdated past the grace window it is a dead claim -> exactly one reclaims it
ok=0
for t in $(seq 1 10); do
  rm -rf "$LOCKS"/work-issue-lock-103*; mkdir "$(lock 103)"; backdate "$(lock 103)"; race 103 $(many)
  [ "$(count 'claimed (reclaimed')" = 1 ] && ok=$((ok + 1)) || bad "old empty dir trial $t: $(cat "$T"/out.* | tr '\n' ' ')"
done
echo "old empty lock dir:    $ok/10 trials with exactly 1 reclaim"
# garbage heartbeat content on an old dir behaves the same
rm -rf "$LOCKS"/work-issue-lock-104*; mkdir "$(lock 104)"; echo junk > "$(lock 104)/heartbeat"; backdate "$(lock 104)"; race 104 "$CLAIM"
[ "$(count 'claimed (reclaimed')" = 1 ] && echo "garbage heartbeat:     reclaimed" || bad "garbage heartbeat not reclaimed"

# 4. live heartbeat: everyone backs off
rm -rf "$(lock 105)"; stale_hb 105 60; race 105 $(many)
[ "$(count live)" = "$RACERS" ] && echo "live heartbeat:        all $RACERS racers live" || bad "live heartbeat not all live"

# 5. ABA: racer A is delayed past B's whole reclaim-and-recreate cycle, then its mv grabs B's fresh claim
sed 's|    STEAL="\$LOCKDIR.stale|    sleep 1; STEAL="$LOCKDIR.stale|' "$CLAIM" > "$T/claim-delayed.sh"
grep -q 'sleep 1; STEAL' "$T/claim-delayed.sh" || bad "ABA setup: sed did not inject the delay"
ok=0; ABA=10
for t in $(seq 1 $ABA); do
  rm -rf "$LOCKS"/work-issue-lock-106*; stale_hb 106
  rm -f "$T"/out.* "$T/go"
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; bash "$T/claim-delayed.sh" 106 > "$T/out.1" 2>&1 ) &
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; sleep 0.1; bash "$CLAIM" 106 > "$T/out.2" 2>&1 ) &
  sleep 0.2; touch "$T/go"; wait
  if [ "$(count 'claimed')" = 1 ] && grep -q "ABA detected" "$T/out.1" && grep -q "claimed (reclaimed" "$T/out.2" && [ -e "$(lock 106)/heartbeat" ]; then ok=$((ok + 1)); else bad "ABA trial $t: $(cat "$T"/out.* | tr '\n' ' ')"; fi
done
echo "ABA (delayed racer):   $ok/$ABA trials: 1 claim, delayed racer detected ABA and restored the lock"

# 6. bad argument must not touch any lock
bash "$CLAIM" "" 2>&1 | grep -q '^RESULT=error' && echo "empty argument:       rejected" || bad "empty argument accepted"

# 7. heartbeat.sh: refreshes an existing lock, and on a missing lock fails without creating it
HB_SH="$HERE/heartbeat.sh"
rm -rf "$LOCKS"/work-issue-lock-107*; stale_hb 107
( cd "$T/repo" && bash "$HB_SH" 107 ) >/dev/null 2>&1; now=$(date +%s); hb=$(cat "$(lock 107)/heartbeat")
[ $((now - hb)) -lt 5 ] && echo "heartbeat.sh refresh:  ok" || bad "heartbeat.sh did not refresh"
rm -rf "$(lock 108)"
( cd "$T/repo" && bash "$HB_SH" 108 ) >/dev/null 2>&1; rc=$?
[ "$rc" != 0 ] && [ ! -e "$(lock 108)" ] && echo "heartbeat.sh no lock:  failed (rc=$rc), lock dir not created" || bad "heartbeat.sh on a missing lock rc=$rc or created the dir"

[ "$contended" -gt 0 ] && echo "contention observed in $contended/$STALE_TRIALS stale trials" || echo "WARNING: no stale trial saw a loser; the steal path was not exercised"
[ "$fail" = 0 ] && echo "ALL PASS" || { echo "$fail FAILURE(s)"; exit 1; }
