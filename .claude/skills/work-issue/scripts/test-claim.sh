#!/bin/bash
# test-claim.sh -- re-run the claim.sh race tests (#35, #72, #276, #281, #297). Run it after ANY
# edit to claim.sh or heartbeat.sh:   bash .claude/skills/work-issue/scripts/test-claim.sh
#
# Everything happens in a throwaway git repo, so no real work-issue lock is touched.
# Racers wait on a start barrier (a `go` file) so they really overlap; each trial
# asserts exactly one winner. macOS (BSD stat/date/touch), like claim.sh.
#
# Counts: FRESH_TRIALS (default 30), STALE_TRIALS (default 20) and MIXED_TRIALS (default 10);
# RACERS per trial defaults to 4. CLAIM=<path> tests another copy of the script: the
# generic sections and the transit-window test run on any copy (the pre-#297 move-based
# script FAILS the transit test: that is the #297 double claim, reproduced on demand);
# the generation-specific tests run only on a copy that uses gen.K dirs. OLD_REF (default
# 6a4cd71, the #281 scripts) is the commit whose claim.sh/heartbeat.sh stand in for the
# older copies agents still run, for the compatibility tests. TRACE=1 prints every racer's
# output for each stale trial whose first attempt had no winner.
#
# The stale-race numbers depend on load: a quiet machine rarely shows a failure in 20
# trials; for a real measurement run 300+ STALE_TRIALS under load (scripts/load-run.sh).
# A failed stale trial is labelled `zero` (nobody claimed) or `double` (two claims).

HERE="$(cd "$(dirname "$0")" && pwd)"
CLAIM="${CLAIM:-$HERE/claim.sh}"
HB_SH="$HERE/heartbeat.sh"
FRESH_TRIALS="${FRESH_TRIALS:-30}"; STALE_TRIALS="${STALE_TRIALS:-20}"; RACERS="${RACERS:-4}"
MIXED_TRIALS="${MIXED_TRIALS:-10}"; OLD_REF="${OLD_REF:-6a4cd71}"
GEN=0; grep -q 'gen\.' "$CLAIM" && GEN=1
T="$(mktemp -d "${TMPDIR:-/tmp}/claim-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
git init -q "$T/repo" || exit 1
LOCKS="$T/repo/.git"
fail=0; contended=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }

# race <issue> <script> [<script> ...]: launch every script at once, collect output in $T/out.*.
# A script given as "<delay>@<path>" starts that many seconds after the barrier.
race() {
  local n="$1"; shift; rm -f "$T"/out.* "$T/go"; local i=0 d p
  for s in "$@"; do
    i=$((i + 1)); d=0; p="$s"
    case "$s" in *@*) d="${s%%@*}"; p="${s#*@}" ;; esac
    ( cd "$T/repo" && while [ ! -e "$T/go" ]; do :; done; [ "$d" = 0 ] || sleep "$d"; bash "$p" "$n" > "$T/out.$i" 2>&1 ) &
  done
  sleep 0.2; touch "$T/go"; wait
}
count() { cat "$T"/out.* | grep -c "^RESULT=$1"; }
trace() { cat "$T"/out.* | tr '\n' '|'; }
lock() { echo "$LOCKS/work-issue-lock-$1"; }
stale_hb() { mkdir "$(lock "$1")"; echo $(( $(date +%s) - ${2:-3000} )) > "$(lock "$1")/heartbeat"; backdate "$(lock "$1")"; }
backdate() { touch -t "$(date -v-60S +%Y%m%d%H%M.%S)" "$1"; }
gens() { ls "$(lock "$1")" 2>/dev/null | grep -cE '^gen\.[0-9]+$'; }
many() { local s=(); local k; for k in $(seq 1 "$RACERS"); do s+=("${1:-$CLAIM}"); done; echo "${s[@]}"; }
# held <issue>: the lock dir ends holding a numeric heartbeat under 60 s old
held() { local h; h=$(cat "$(lock "$1")/heartbeat" 2>/dev/null); printf '%s' "$h" | grep -qE '^[0-9]+$' && [ $(( $(date +%s) - h )) -lt 60 ]; }
# inject <regex> <replacement> <src> <out>: a delay injected into a COPY of a script
inject() { sed -E "s/$1/$2/" "$3" > "$4"; cmp -s "$3" "$4" && bad "setup: injection not applied to $4"; }

# 1. fresh claims: one mkdir winner, the rest live (the empty-dir grace window) or lost
ok=0
for t in $(seq 1 "$FRESH_TRIALS"); do
  rm -rf "$(lock 100)"; race 100 $(many)
  c=$(count 'claimed (fresh)'); o=$(( $(count 'live') + $(count 'lost') ))
  if [ "$c" = 1 ] && [ $((c + o)) = "$RACERS" ]; then ok=$((ok + 1)); else bad "fresh trial $t: claimed=$c other=$o"; fi
done
echo "fresh races:           $ok/$FRESH_TRIALS trials with exactly 1 winner ($RACERS racers each)"

# 2. stale-heartbeat reclaims: EXACTLY one winner, and the lock ends held. Two winners is the
#    safety failure (#297); zero is the liveness failure (#281). With the generation lock a
#    reclaim race creates exactly one gen dir.
ok=0; lost=0; zero=0; double=0; zero1=0; retried=0; reclaimed=0; fresh2=0
for t in $(seq 1 "$STALE_TRIALS"); do
  rm -rf "$LOCKS"/work-issue-lock-101*; stale_hb 101; race 101 $(many)
  c=$(count 'claimed'); l=$(count 'lost'); lv=$(count 'live')
  reclaimed=$((reclaimed + $(count 'claimed (reclaimed'))); fresh2=$((fresh2 + $(count 'claimed (fresh)')))
  lost=$((lost + l)); [ $((l + lv)) -gt 0 ] && contended=$((contended + 1))
  # first attempt had a winner iff some claim did not need a retry (only the #281 script retries)
  c1=$(cat "$T"/out.* | grep '^RESULT=claimed' | grep -vc '\[attempt ')
  [ "$c1" = 0 ] && { zero1=$((zero1 + 1)); [ -n "$TRACE" ] && echo "trace (first attempt no winner) $t: $(trace)"; }
  cat "$T"/out.* | grep -q '^claim.sh: attempt ' && retried=$((retried + 1))
  g_ok=1; [ "$GEN" = 1 ] && [ "$(gens 101)" != 1 ] && g_ok=0
  if [ "$c" = 1 ] && [ $((c + l + lv)) = "$RACERS" ] && held 101 && [ "$g_ok" = 1 ]; then ok=$((ok + 1))
  elif [ "$c" = 0 ]; then zero=$((zero + 1)); bad "stale trial $t (zero): claimed=0 lost=$l live=$lv :: $(trace)"
  elif [ "$c" -gt 1 ]; then double=$((double + 1)); bad "stale trial $t (double): claimed=$c lost=$l live=$lv :: $(trace)"
  else bad "stale trial $t (other): claimed=$c lost=$l live=$lv held=$(held 101 && echo y || echo n) gens=$(gens 101) :: $(trace)"; fi
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
# ... and backdated past the grace window it is a dead claim -> exactly one racer takes it.
ok=0; f3=0
for t in $(seq 1 10); do
  rm -rf "$LOCKS"/work-issue-lock-103*; mkdir "$(lock 103)"; backdate "$(lock 103)"; race 103 $(many)
  f3=$((f3 + $(count 'claimed (fresh)')))
  [ "$(count 'claimed')" = 1 ] && held 103 && ok=$((ok + 1)) || bad "old empty dir trial $t: $(trace)"
done
echo "old empty lock dir:    $ok/10 trials with exactly 1 winner ($f3 fresh-after-steal)"
# garbage heartbeat content on an old dir behaves the same
rm -rf "$LOCKS"/work-issue-lock-104*; mkdir "$(lock 104)"; echo junk > "$(lock 104)/heartbeat"; backdate "$(lock 104)"; race 104 "$CLAIM"
[ "$(count 'claimed (reclaimed')" = 1 ] && echo "garbage heartbeat:     reclaimed" || bad "garbage heartbeat not reclaimed"

# 4. live heartbeat: everyone backs off
rm -rf "$(lock 105)"; stale_hb 105 60; race 105 $(many)
[ "$(count live)" = "$RACERS" ] && echo "live heartbeat:        all $RACERS racers live" || bad "live heartbeat not all live"

# 5. (#297) the transit window, forced. Racer A decides the lock is stale, then is delayed 1 s;
#    racer B (+0.1 s) reclaims it; a newcomer C arrives at +1.5 s. For the move-based script A's
#    delayed mv carries B's fresh lock away and A holds it away 1 s more before restoring, so C's
#    fresh mkdir lands in the freed name and the restore nests into C's dir: two claims (fails
#    here, by design, for CLAIM=<pre-#297 copy>). The generation script moves nothing: A's
#    mkdir of gen.1 finds B's and fails, and C sees B's young generation and reads live.
if [ "$GEN" = 1 ]; then
  inject '^( *)NEXT=\$\(\( K \+ 1 \)\)' '\1sleep 1; NEXT=$(( K + 1 ))' "$CLAIM" "$T/claim-a.sh"
else
  inject '((el)?if) mv "\$LOCKDIR" "\$STEAL"' '\1 sleep 1 \&\& mv "$LOCKDIR" "$STEAL"' "$CLAIM" "$T/claim-a0.sh"
  inject '^( *)mv "\$STEAL" "\$LOCKDIR"' '\1sleep 1; mv "$STEAL" "$LOCKDIR"' "$T/claim-a0.sh" "$T/claim-a.sh"
fi
ok=0; dbl=0
for t in $(seq 1 5); do
  rm -rf "$LOCKS"/work-issue-lock-106*; stale_hb 106
  race 106 "$T/claim-a.sh" "0.1@$CLAIM" "1.5@$CLAIM"
  if [ "$(count 'claimed')" = 1 ] && grep -q '^RESULT=claimed (reclaimed' "$T/out.2" && held 106; then ok=$((ok + 1))
  else [ "$(count 'claimed')" -gt 1 ] && dbl=$((dbl + 1)); bad "transit trial $t: $(trace)"; fi
done
echo "transit window:        $ok/5 trials with exactly 1 claim ($dbl double)"

if [ "$GEN" = 1 ]; then
  # 6. (#297) a reclaim in flight: B took gen.1 but stalls 2 s before its heartbeat, so the
  #    heartbeat still reads stale. C (+0.5 s) must read the young generation as live.
  inject '^( *)if \[ "\$\(inode\)" = "\$I0" \] && write_hb' '\1sleep 2; if [ "$(inode)" = "$I0" ] \&\& write_hb' "$CLAIM" "$T/claim-slowhb.sh"
  ok=0
  for t in $(seq 1 3); do
    rm -rf "$LOCKS"/work-issue-lock-107*; stale_hb 107
    race 107 "$T/claim-slowhb.sh" "0.5@$CLAIM"
    grep -q '^RESULT=claimed (reclaimed' "$T/out.1" && grep -q '^RESULT=live (a claim is in flight' "$T/out.2" && held 107 && ok=$((ok + 1)) || bad "in-flight gen trial $t: $(trace)"
  done
  echo "in-flight generation:  $ok/3 trials: the newcomer read the young gen as live"

  # 7. the container replaced under a delayed racer: A decides, stalls 1 s; meanwhile the lock
  #    is released (rm -rf) and C claims it fresh. A's gen.1 lands in C's container: A must see
  #    the inode change and report lost.
  ok=0
  for t in $(seq 1 3); do
    rm -rf "$LOCKS"/work-issue-lock-108*; stale_hb 108
    ( sleep 0.5; rm -rf "$(lock 108)" ) &
    race 108 "$T/claim-a.sh" "0.7@$CLAIM"; wait
    grep -q '^RESULT=lost the reclaim race (the lock changed' "$T/out.1" && grep -q '^RESULT=claimed (fresh)' "$T/out.2" && held 108 && ok=$((ok + 1)) || bad "replaced container trial $t: $(trace)"
  done
  echo "replaced container:    $ok/3 trials: the delayed racer saw the new inode and lost"

  # 8. reclaimed past us: B stalls 2 s after taking gen.1 and someone takes gen.2 meanwhile
  #    (what a reclaim after a >10 s stall looks like). B must not report claimed.
  rm -rf "$LOCKS"/work-issue-lock-109*; stale_hb 109
  ( sleep 1; mkdir "$(lock 109)/gen.2" ) &
  race 109 "$T/claim-slowhb.sh"; wait
  grep -q '^RESULT=lost the reclaim race (the lock changed' "$T/out.1" && echo "reclaimed past us:     lost" || bad "reclaimed past us: $(trace)"

  # 9. generations: the newest is found numerically (gen.10 > gen.9), non-generation entries
  #    (an older script's nested ABA leftover, gen.x) are ignored, and the reclaim takes gen.11.
  rm -rf "$LOCKS"/work-issue-lock-110*; stale_hb 110
  for k in 1 2 9 10; do mkdir "$(lock 110)/gen.$k"; backdate "$(lock 110)/gen.$k"; done
  mkdir "$(lock 110)/gen.x" "$(lock 110)/work-issue-lock-110.stale.1.1.1"; backdate "$(lock 110)"
  race 110 "$CLAIM"
  grep -q '^RESULT=claimed (reclaimed' "$T/out.1" && [ -d "$(lock 110)/gen.11" ] && [ "$(gens 110)" = 5 ] && held 110 \
    && echo "newest generation:     gen.10 -> gen.11, junk ignored" || bad "newest generation: $(trace) $(ls "$(lock 110)")"
  # ... and a young newest generation with a stale heartbeat is live
  rm -rf "$LOCKS"/work-issue-lock-111*; stale_hb 111; mkdir "$(lock 111)/gen.1"; backdate "$(lock 111)/gen.1"; mkdir "$(lock 111)/gen.2"
  race 111 "$CLAIM"
  grep -q '^RESULT=live (a claim is in flight' "$T/out.1" && echo "young gen.2:           live" || bad "young gen.2: $(trace)"

  # 10. Compatibility with the older scripts agents still run (claim-protocol.md "Rollout").
  OLD="$T/old"; mkdir "$OLD"
  if git -C "$HERE" show "$OLD_REF:.claude/skills/work-issue/scripts/claim.sh" > "$OLD/claim.sh" 2>/dev/null \
     && git -C "$HERE" show "$OLD_REF:.claude/skills/work-issue/scripts/heartbeat.sh" > "$OLD/heartbeat.sh" 2>/dev/null; then
    # an old-format stale lock (what work-issue-lock-113/-120/-169 look like): reclaimed as gen.1
    rm -rf "$LOCKS"/work-issue-lock-112*; mkdir "$(lock 112)"; echo 1788668669 > "$(lock 112)/heartbeat"
    touch -t 202509052124 "$(lock 112)/heartbeat" "$(lock 112)"
    race 112 $(many)
    [ "$(count 'claimed (reclaimed')" = 1 ] && [ "$(count 'claimed')" = 1 ] && [ "$(gens 112)" = 1 ] && held 112 \
      && echo "old-format stale lock: reclaimed once, as gen.1" || bad "old-format stale lock: $(trace)"
    # an old heartbeat.sh keeps a gen-layout lock live; the new one works on both layouts
    rm -rf "$LOCKS"/work-issue-lock-113*; stale_hb 113; mkdir "$(lock 113)/gen.1"; backdate "$(lock 113)/gen.1"
    ( cd "$T/repo" && bash "$OLD/heartbeat.sh" 113 ); race 113 "$CLAIM" "$OLD/claim.sh"
    [ "$(count live)" = 2 ] && echo "old heartbeat.sh:      keeps a gen lock live for old and new claim.sh" || bad "old heartbeat.sh on gen lock: $(trace)"
    rm -rf "$LOCKS"/work-issue-lock-114*; stale_hb 114
    ( cd "$T/repo" && bash "$HB_SH" 114 ); race 114 "$OLD/claim.sh"
    [ "$(count live)" = 1 ] && echo "new heartbeat.sh:      keeps an old-format lock live for old claim.sh" || bad "new heartbeat.sh on old lock: $(trace)"
    # an old claim.sh moves a stale gen-layout lock away and re-creates the old format; the
    # new script then reads that as generation 0, live
    rm -rf "$LOCKS"/work-issue-lock-115*; stale_hb 115; mkdir "$(lock 115)/gen.1"; backdate "$(lock 115)/gen.1"; backdate "$(lock 115)"
    race 115 "$OLD/claim.sh"; o1=$(trace); race 115 "$CLAIM"
    printf '%s' "$o1" | grep -q 'RESULT=claimed' && [ "$(count live)" = 1 ] && [ "$(gens 115)" = 0 ] \
      && echo "old claim.sh steal:    old reclaim of a gen lock, then new reads it live" || bad "old steal of gen lock: $o1 $(trace)"
    # release (rm -rf, every version) leaves nothing behind
    rm -rf "$(lock 115)"; [ ! -e "$(lock 115)" ] && echo "release:               rm -rf removes a gen lock" || bad "release left $(lock 115)"
    # mixed stale races, 2 old + 2 new: informational. An old copy still moves the lock, so the
    # transit window survives while old copies run (the rollout residual); report, never hide.
    m_ok=0; m_zero=0; m_dbl=0
    for t in $(seq 1 "$MIXED_TRIALS"); do
      rm -rf "$LOCKS"/work-issue-lock-116*; stale_hb 116; race 116 "$OLD/claim.sh" "$CLAIM" "$OLD/claim.sh" "$CLAIM"
      c=$(count 'claimed')
      if [ "$c" = 1 ]; then m_ok=$((m_ok + 1)); elif [ "$c" = 0 ]; then m_zero=$((m_zero + 1)); else m_dbl=$((m_dbl + 1)); echo "  mixed double $t: $(trace)"; fi
    done
    echo "mixed old/new races:   $m_ok/$MIXED_TRIALS exactly 1 winner ($m_zero zero, $m_dbl double; informational, see Rollout)"
  else
    echo "compatibility tests:   skipped (no $OLD_REF in this repo)"
  fi
fi

# 11. bad argument must not touch any lock
bash "$CLAIM" "" 2>&1 | grep -q '^RESULT=error' && echo "empty argument:        rejected" || bad "empty argument accepted"

# 12. heartbeat.sh: refreshes an existing lock, and on a missing lock fails without creating it
rm -rf "$LOCKS"/work-issue-lock-117*; stale_hb 117
( cd "$T/repo" && bash "$HB_SH" 117 ) >/dev/null 2>&1; now=$(date +%s); hb=$(cat "$(lock 117)/heartbeat")
[ $((now - hb)) -lt 5 ] && echo "heartbeat.sh refresh:  ok" || bad "heartbeat.sh did not refresh"
rm -rf "$(lock 118)"
( cd "$T/repo" && bash "$HB_SH" 118 ) >/dev/null 2>&1; rc=$?
[ "$rc" != 0 ] && [ ! -e "$(lock 118)" ] && echo "heartbeat.sh no lock:  failed (rc=$rc), lock dir not created" || bad "heartbeat.sh on a missing lock rc=$rc or created the dir"

[ "$contended" -gt 0 ] && echo "contention observed in $contended/$STALE_TRIALS stale trials" || echo "WARNING: no stale trial saw a loser; the steal path was not exercised"
[ "$fail" = 0 ] && echo "ALL PASS" || { echo "$fail FAILURE(s)"; exit 1; }
