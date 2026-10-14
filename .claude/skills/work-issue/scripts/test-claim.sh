#!/bin/bash
# test-claim.sh -- re-run the claim.sh race tests (#35, #72, #276, #281). Run it after ANY
# edit to claim.sh:   bash .claude/skills/work-issue/scripts/test-claim.sh
#
# Everything happens in a throwaway git repo, so no real work-issue lock is touched.
# Racers wait on a start barrier (a `go` file) so they really overlap; each trial
# asserts exactly one winner. macOS (BSD stat/date/touch), like claim.sh.
#
# Counts: FRESH_TRIALS (default 30) and STALE_TRIALS (default 20) match the numbers
# the protocol documents; RACERS per trial defaults to 4. CLAIM=<path> tests another
# copy of the script (e.g. the pre-#281 one, for before/after numbers). TRACE=1 prints
# every racer's output for each stale trial whose first attempt had no winner.
#
# The stale-race numbers depend on load: a quiet machine rarely shows a failure in 20
# trials; for a real measurement run 300+ STALE_TRIALS while the machine is busy.
# A failed stale trial is labelled `zero` (nobody claimed: the #281 liveness bug) or
# `double` (two claims: the transit-window residual, #297).

HERE="$(cd "$(dirname "$0")" && pwd)"
CLAIM="${CLAIM:-$HERE/claim.sh}"
FRESH_TRIALS="${FRESH_TRIALS:-30}"; STALE_TRIALS="${STALE_TRIALS:-20}"; RACERS="${RACERS:-4}"
T="$(mktemp -d "${TMPDIR:-/tmp}/claim-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
git init -q "$T/repo" || exit 1
LOCKS="$T/repo/.git"
fail=0; contended=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }

# race <issue> <script> [<script> ...]: launch every script at once, collect output in $T/out.*
race() {
  local n="$1"; shift; rm -f "$T"/out.* "$T/go"; local i=0
  for s in "$@"; do
    i=$((i + 1))
    ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; bash "$s" "$n" > "$T/out.$i" 2>&1 ) &
  done
  sleep 0.2; touch "$T/go"; wait
}
count() { cat "$T"/out.* | grep -c "^RESULT=$1"; }
trace() { cat "$T"/out.* | tr '\n' '|'; }
lock() { echo "$LOCKS/work-issue-lock-$1"; }
stale_hb() { mkdir "$(lock "$1")"; echo $(( $(date +%s) - ${2:-3000} )) > "$(lock "$1")/heartbeat"; }
backdate() { touch -t "$(date -v-60S +%Y%m%d%H%M.%S)" "$1"; }
many() { local s=(); local k; for k in $(seq 1 "$RACERS"); do s+=("${1:-$CLAIM}"); done; echo "${s[@]}"; }
# held <issue>: the lock dir ends holding a numeric heartbeat under 60 s old
held() { local h; h=$(cat "$(lock "$1")/heartbeat" 2>/dev/null); printf '%s' "$h" | grep -qE '^[0-9]+$' && [ $(( $(date +%s) - h )) -lt 60 ]; }
# Delay injections into a COPY of claim.sh. Both work on the pre-#281 script too.
#   delay_steal <secs> <out>: sleep just before the stale-steal mv (after the #281 inode pre-check)
#   delay_decide <secs> <out>: sleep just before STEAL= (before the pre-check)
#   delay_recreate <secs> <file>: sleep between the reclaim branch's mkdir and its heartbeat write
delay_steal() { sed -E 's/((el)?if) mv "\$LOCKDIR" "\$STEAL"/\1 sleep '"$1"' \&\& mv "$LOCKDIR" "$STEAL"/' "$CLAIM" > "$2"; grep -q "sleep $1 && mv" "$2" || bad "setup: delay_steal not injected"; }
delay_decide() { sed -E 's/^( *)STEAL="\$LOCKDIR.stale/\1sleep '"$1"'; STEAL="$LOCKDIR.stale/' "$CLAIM" > "$2"; grep -q "sleep $1; STEAL" "$2" || bad "setup: delay_decide not injected"; }
delay_recreate() {
  local l; l=$(grep -n 'mkdir "$LOCKDIR" 2>/dev/null && date' "$2" | sed -n 2p | cut -d: -f1)
  [ -n "$l" ] && sed -i '' "${l}s/2>\/dev\/null && date/2>\/dev\/null \&\& sleep $1 \&\& date/" "$2"
  grep -q "sleep $1 && date" "$2" || bad "setup: delay_recreate not injected"
}

# 1. fresh claims: one mkdir winner, the rest live (the empty-dir grace window) or lost
ok=0
for t in $(seq 1 "$FRESH_TRIALS"); do
  rm -rf "$(lock 100)"; race 100 $(many)
  c=$(count 'claimed (fresh)'); o=$(( $(count 'live') + $(count 'lost') ))
  if [ "$c" = 1 ] && [ $((c + o)) = "$RACERS" ]; then ok=$((ok + 1)); else bad "fresh trial $t: claimed=$c other=$o"; fi
done
echo "fresh races:           $ok/$FRESH_TRIALS trials with exactly 1 winner ($RACERS racers each)"

# 2. stale-heartbeat reclaims: EXACTLY one winner, and the lock ends held. Two winners is the
#    safety failure; zero is the #281 liveness failure (a racer's ABA restore lands on the
#    stealer's in-flight re-create; since #281 that stealer re-checks and reclaims the orphan).
ok=0; lost=0; zero=0; double=0; zero1=0; retried=0; reclaimed=0; fresh2=0
for t in $(seq 1 "$STALE_TRIALS"); do
  rm -rf "$LOCKS"/work-issue-lock-101*; stale_hb 101; race 101 $(many)
  c=$(count 'claimed'); l=$(count 'lost'); lv=$(count 'live')
  reclaimed=$((reclaimed + $(count 'claimed (reclaimed'))); fresh2=$((fresh2 + $(count 'claimed (fresh)')))
  lost=$((lost + l)); [ $((l + lv)) -gt 0 ] && contended=$((contended + 1))
  # first attempt had a winner iff some claim did not need a retry
  c1=$(cat "$T"/out.* | grep '^RESULT=claimed' | grep -vc '\[attempt ')
  [ "$c1" = 0 ] && { zero1=$((zero1 + 1)); [ -n "$TRACE" ] && echo "trace (first attempt no winner) $t: $(trace)"; }
  cat "$T"/out.* | grep -q '^claim.sh: attempt ' && retried=$((retried + 1))
  if [ "$c" = 1 ] && [ $((c + l + lv)) = "$RACERS" ] && held 101; then ok=$((ok + 1))
  elif [ "$c" = 0 ]; then zero=$((zero + 1)); bad "stale trial $t (zero): claimed=0 lost=$l live=$lv :: $(trace)"
  elif [ "$c" -gt 1 ]; then double=$((double + 1)); bad "stale trial $t (double): claimed=$c lost=$l live=$lv :: $(trace)"
  else bad "stale trial $t (other): claimed=$c lost=$l live=$lv held=$(held 101 && echo y || echo n) :: $(trace)"; fi
done
echo "stale-reclaim races:   $ok/$STALE_TRIALS trials with exactly 1 winner ($zero zero-winner, $double double-claim; $reclaimed reclaimed, $fresh2 fresh-after-steal; $lost racers reported 'lost')"
echo "  first attempt:       $zero1 trials with no winner on the first attempt; $retried trials needed a retry"

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
  [ "$(count 'claimed')" = 1 ] && [ "$(count 'claimed (reclaimed')" = 1 ] && ok=$((ok + 1)) || bad "old empty dir trial $t: $(trace)"
done
echo "old empty lock dir:    $ok/10 trials with exactly 1 reclaim"
# garbage heartbeat content on an old dir behaves the same
rm -rf "$LOCKS"/work-issue-lock-104*; mkdir "$(lock 104)"; echo junk > "$(lock 104)/heartbeat"; backdate "$(lock 104)"; race 104 "$CLAIM"
[ "$(count 'claimed (reclaimed')" = 1 ] && echo "garbage heartbeat:     reclaimed" || bad "garbage heartbeat not reclaimed"

# 4. live heartbeat: everyone backs off
rm -rf "$(lock 105)"; stale_hb 105 60; race 105 $(many)
[ "$(count live)" = "$RACERS" ] && echo "live heartbeat:        all $RACERS racers live" || bad "live heartbeat not all live"

# 5. ABA: racer A is delayed (just before its mv) past B's whole reclaim-and-recreate cycle,
#    so its mv grabs B's fresh claim; it must notice, restore, and report it.
delay_steal 1 "$T/claim-delayed.sh"
ok=0; ABA=10
for t in $(seq 1 $ABA); do
  rm -rf "$LOCKS"/work-issue-lock-106*; stale_hb 106
  rm -f "$T"/out.* "$T/go"
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; bash "$T/claim-delayed.sh" 106 > "$T/out.1" 2>&1 ) &
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; sleep 0.1; bash "$CLAIM" 106 > "$T/out.2" 2>&1 ) &
  sleep 0.2; touch "$T/go"; wait
  if [ "$(count 'claimed')" = 1 ] && grep -q "ABA detected" "$T/out.1" && grep -q "claimed (reclaimed" "$T/out.2" && held 106; then ok=$((ok + 1)); else bad "ABA trial $t: $(trace)"; fi
done
echo "ABA (delayed racer):   $ok/$ABA trials: 1 claim, delayed racer detected ABA and restored the lock"

# 5b. (#281) the same delay BEFORE the inode pre-check: the racer sees the lock was replaced
#     and backs off without moving anything (no ABA move, so no moment with the name free)
delay_decide 1 "$T/claim-late.sh"
ok=0
for t in $(seq 1 5); do
  rm -rf "$LOCKS"/work-issue-lock-109*; stale_hb 109
  rm -f "$T"/out.* "$T/go"
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; bash "$T/claim-late.sh" 109 > "$T/out.1" 2>&1 ) &
  ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; sleep 0.1; bash "$CLAIM" 109 > "$T/out.2" 2>&1 ) &
  sleep 0.2; touch "$T/go"; wait
  if [ "$(count 'claimed')" = 1 ] && grep -q "replaced before the steal" "$T/out.1" && held 109; then ok=$((ok + 1)); else bad "pre-check trial $t: $(trace)"; fi
done
echo "inode pre-check:       $ok/5 trials: late racer backed off without moving the lock"

# 5c. (#281) empty-dir ABA: on an old EMPTY lock both racers read heartbeat "", so the content
#     check alone cannot tell the stale dir from R1's freshly re-created (still empty) one.
#     R1 steals at +0.2 s and pauses 1 s before its heartbeat; R2 passes its pre-check at once
#     but moves at +0.6 s, grabbing R1's empty dir. The inode check must catch it (before #281
#     R2 "reclaimed" it and R1's heartbeat then landed in R2's dir: a double claim).
delay_steal 0.2 "$T/claim-r1.sh"; delay_recreate 1 "$T/claim-r1.sh"; delay_steal 0.6 "$T/claim-r2.sh"
ok=0
for t in $(seq 1 5); do
  rm -rf "$LOCKS"/work-issue-lock-110*; mkdir "$(lock 110)"; backdate "$(lock 110)"
  race 110 "$T/claim-r1.sh" "$T/claim-r2.sh"
  if [ "$(count 'claimed')" = 1 ] && grep -q "^RESULT=claimed (reclaimed" "$T/out.1" && grep -q "ABA detected" "$T/out.2" && held 110; then ok=$((ok + 1)); else bad "empty-dir ABA trial $t: $(trace)"; fi
done
echo "empty-dir ABA:         $ok/5 trials: 1 claim, the inode check caught the move"

# 5d. (#281) the 3-way orphan: a stealer's re-create was knocked away and the lock left with
#     no heartbeat. Simulated directly: an empty dir 3 s old, and a script copy forced into
#     the 3-way branch on its first attempt. It must wait out the grace window and reclaim.
sed 's/^  RETRY=0$/  RETRY=0; [ "${ATTEMPT:-1}" = 1 ] \&\& { RES="lost a rare 3-way reclaim race (simulated)"; RETRY=1; return; }/' "$CLAIM" > "$T/claim-3way.sh"
if grep -q 'simulated' "$T/claim-3way.sh"; then
  rm -rf "$LOCKS"/work-issue-lock-111*; mkdir "$(lock 111)"; touch -t "$(date -v-3S +%Y%m%d%H%M.%S)" "$(lock 111)"
  s0=$(date +%s); race 111 "$T/claim-3way.sh"; el=$(( $(date +%s) - s0 ))
  if grep -q "^RESULT=claimed (reclaimed a stale lock) \[attempt 2\]" "$T/out.1" && grep -q '^claim.sh: attempt 1: lost' "$T/out.1" && [ "$el" -ge 6 ] && held 111; then
    echo "3-way orphan:          reclaimed on attempt 2 after ${el}s (waited out the grace window)"
  else bad "3-way orphan: el=${el}s $(trace)"; fi
  # ... and if someone else claims meanwhile, the retry reports live instead
  rm -rf "$LOCKS"/work-issue-lock-112*; mkdir "$(lock 112)"
  ( sleep 1; date +%s > "$(lock 112)/heartbeat" ) &
  race 112 "$T/claim-3way.sh"; wait
  grep -q "^RESULT=live .*\[attempt 2\]" "$T/out.1" && echo "3-way, then claimed:   retry reports live" || bad "3-way then claimed: $(trace)"
else
  echo "3-way orphan:          skipped (CLAIM has no retry)"
fi

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
