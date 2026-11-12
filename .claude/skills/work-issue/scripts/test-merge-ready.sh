#!/bin/bash
# test-merge-ready.sh -- check the merge-readiness jq expression in SKILL.md 8.12 (#709).
# Run it after ANY edit to that expression:
#   bash .claude/skills/work-issue/scripts/test-merge-ready.sh
#   LIVE_PR=<PR#> bash .claude/skills/work-issue/scripts/test-merge-ready.sh   # also run it through `gh` (gojq)
#
# It reads the expression from SKILL.md itself (so the test covers the copy the skill
# runs) and applies it to fixture `gh pr view --json mergeable,mergeStateStatus,
# statusCheckRollup` documents. The owner's rule (2026-11-12):
#   * SUCCESS and SKIPPED pass.
#   * NEUTRAL passes only for the check named `CodeQL`, and only when at least one
#     `Analyze (...)` check exists and every one of them is SUCCESS (CodeQL's default setup
#     rescans main on every push and weekly, so GitHub does the later check, not us).
#   * Any other NEUTRAL, CANCELLED, FAILURE, IN_PROGRESS or empty/null conclusion blocks.
#   * An empty rollup blocks (jq's `all` is true on []).
# No network unless LIVE_PR is set. The fixtures run under system jq; the live run goes
# through `gh --jq` (gojq), which is the path the skill really uses.

HERE="$(cd "$(dirname "$0")" && pwd)"
SKILL="$HERE/../SKILL.md"
fail=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }

# The expression is the single-quoted jq program after `statusCheckRollup --jq '` on one line.
LINE="$(grep -F 'statusCheckRollup --jq ' "$SKILL" | head -1)"
EXPR="${LINE#*statusCheckRollup --jq \'}"
EXPR="${EXPR%\'*}"
if [ -z "$LINE" ] || [ -z "$EXPR" ]; then echo "FAIL: no merge-readiness expression found in $SKILL"; exit 1; fi

# mk MERGEABLE STATE ENTRY... -> a fixture document. ENTRY is one of
#   name=CONCLUSION            a completed CheckRun
#   name=@STATUS/CONCLUSION    a CheckRun with an explicit status and a conclusion ("" or null allowed)
#   ctx#STATE                  a StatusContext
mk() {
    local mergeable=$1 state=$2; shift 2
    local out="[]" e
    for e in "$@"; do
        case "$e" in
            *'#'*) out=$(jq -c --arg c "${e%%#*}" --arg s "${e#*#}" '. + [{__typename:"StatusContext",context:$c,state:$s}]' <<<"$out") ;;
            *'=@'*) local n=${e%%=@*} rest=${e#*=@}
                    out=$(jq -c --arg n "$n" --arg st "${rest%%/*}" --arg c "${rest#*/}" \
                        '. + [{__typename:"CheckRun",name:$n,status:$st,conclusion:(if $c == "null" then null else $c end)}]' <<<"$out") ;;
            *) out=$(jq -c --arg n "${e%%=*}" --arg c "${e#*=}" '. + [{__typename:"CheckRun",name:$n,status:"COMPLETED",conclusion:$c}]' <<<"$out") ;;
        esac
    done
    jq -nc --arg m "$mergeable" --arg s "$state" --argjson r "$out" '{mergeable:$m,mergeStateStatus:$s,statusCheckRollup:$r}'
}

check() { # check WANT DESCRIPTION DOC
    local got
    got="$(printf '%s' "$3" | jq "$EXPR" 2>&1)"
    [ "$got" = "$1" ] || bad "$2: got '$got', want '$1'"
}

OK=(test=SUCCESS markdown-links=SUCCESS)
AN=("Analyze (rust)=SUCCESS" "Analyze (python)=SUCCESS" "Analyze (actions)=SUCCESS")

check true  "all SUCCESS"                          "$(mk MERGEABLE CLEAN "${OK[@]}")"
check true  "SUCCESS plus SKIPPED"                 "$(mk MERGEABLE CLEAN "${OK[@]}" draft=SKIPPED publish=SKIPPED)"
check true  "only SKIPPED beside one SUCCESS"      "$(mk MERGEABLE CLEAN test=SUCCESS draft=SKIPPED)"
check true  "CodeQL NEUTRAL, every Analyze SUCCESS" "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" CodeQL=NEUTRAL)"
check true  "CodeQL SUCCESS"                       "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" CodeQL=SUCCESS)"
check false "CodeQL NEUTRAL, an Analyze FAILURE"   "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" "Analyze (go)=FAILURE" CodeQL=NEUTRAL)"
check false "CodeQL NEUTRAL, an Analyze NEUTRAL"   "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" "Analyze (go)=NEUTRAL" CodeQL=NEUTRAL)"
check false "CodeQL NEUTRAL, an Analyze IN_PROGRESS" "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" "Analyze (go)=@IN_PROGRESS/" CodeQL=NEUTRAL)"
check false "CodeQL NEUTRAL, no Analyze check at all" "$(mk MERGEABLE CLEAN "${OK[@]}" CodeQL=NEUTRAL)"
check false "another check NEUTRAL"                "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" lint=NEUTRAL)"
check false "another check NEUTRAL beside a CodeQL NEUTRAL" "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" CodeQL=NEUTRAL lint=NEUTRAL)"
check false "Analyze NEUTRAL without CodeQL (only CodeQL is exempt)" "$(mk MERGEABLE CLEAN "${OK[@]}" "Analyze (rust)=NEUTRAL")"
check false "a lookalike name is not CodeQL"       "$(mk MERGEABLE CLEAN "${OK[@]}" "${AN[@]}" "CodeQL-extra=NEUTRAL")"
check false "CANCELLED"                            "$(mk MERGEABLE CLEAN "${OK[@]}" build=CANCELLED)"
check false "FAILURE"                              "$(mk MERGEABLE CLEAN "${OK[@]}" build=FAILURE)"
check false "IN_PROGRESS with conclusion \"\""     "$(mk MERGEABLE CLEAN "${OK[@]}" "test2=@IN_PROGRESS/")"
check false "IN_PROGRESS with conclusion null"     "$(mk MERGEABLE CLEAN "${OK[@]}" "test2=@IN_PROGRESS/null")"
check false "QUEUED with conclusion null"          "$(mk MERGEABLE CLEAN "${OK[@]}" "test2=@QUEUED/null")"
check false "empty rollup"                         "$(mk MERGEABLE CLEAN)"
check false "DIRTY with green checks"              "$(mk MERGEABLE DIRTY "${OK[@]}")"
check false "mergeable UNKNOWN"                    "$(mk UNKNOWN UNKNOWN "${OK[@]}")"
check false "CONFLICTING"                          "$(mk CONFLICTING DIRTY "${OK[@]}")"
check true  "BEHIND does not block"                "$(mk MERGEABLE BEHIND "${OK[@]}")"
check true  "StatusContext SUCCESS (no .name)"     "$(mk MERGEABLE CLEAN "${OK[@]}" "ci/legacy#SUCCESS")"
check false "StatusContext PENDING"                "$(mk MERGEABLE CLEAN "${OK[@]}" "ci/legacy#PENDING")"
check false "StatusContext FAILURE"                "$(mk MERGEABLE CLEAN "${OK[@]}" "ci/legacy#FAILURE")"

# A PR #1137-shaped rollup (a release PR: three skipped release jobs, CodeQL green).
R1137=(test=SUCCESS markdown-links=SUCCESS "Analyze (rust)=SUCCESS" "Analyze (python)=SUCCESS" draft=SKIPPED publish=SKIPPED post-publish-check=SKIPPED packaging=SUCCESS CodeQL=SUCCESS)
check true  "PR #1137 shape (skipped release jobs)" "$(mk MERGEABLE CLEAN "${R1137[@]}")"
# The old expression, for the record: it printed false on that shape.
OLD='(.mergeable == "MERGEABLE" and .mergeStateStatus != "DIRTY") and ([.statusCheckRollup[] | (.conclusion // .state)] | (length > 0 and all(. == "SUCCESS")))'
got="$(mk MERGEABLE CLEAN "${R1137[@]}" | jq "$OLD")"
[ "$got" = false ] || bad "old expression on the PR #1137 shape: got '$got', want false (the fixture no longer shows the bug)"

# Live run through gh (gojq): the same expression must agree with system jq on a real PR.
if [ -n "${LIVE_PR:-}" ]; then
    live="$(gh pr view "$LIVE_PR" --json mergeable,mergeStateStatus,statusCheckRollup --jq "$EXPR" 2>&1)"
    sysjq="$(gh pr view "$LIVE_PR" --json mergeable,mergeStateStatus,statusCheckRollup | jq "$EXPR" 2>&1)"
    echo "live PR #$LIVE_PR: gh --jq (gojq) -> $live; system jq -> $sysjq"
    [ "$live" = "$sysjq" ] || bad "gojq and system jq disagree on PR #$LIVE_PR"
    case "$live" in true|false) ;; *) bad "live run did not print true/false: $live" ;; esac
fi

if [ "$fail" -eq 0 ]; then echo "PASS: merge-readiness expression: SKIPPED passes, only CodeQL NEUTRAL passes (with every Analyze SUCCESS), the rest blocks"; exit 0; fi
echo "$fail check(s) failed"; exit 1
