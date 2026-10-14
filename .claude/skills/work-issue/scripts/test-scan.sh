#!/bin/bash
# test-scan.sh -- check the open-PR claim-scan jq expression in SKILL.md (#293).
# Run it after ANY edit to that expression:
#   bash .claude/skills/work-issue/scripts/test-scan.sh
#
# It reads the expression from SKILL.md itself (so the test covers the copy the
# skill runs), applies it to a fixture of PR bodies with multi-digit numbers, and
# asserts the exact output. No network: `gh` is not called.

HERE="$(cd "$(dirname "$0")" && pwd)"
SKILL="$HERE/../SKILL.md"
fail=0
bad() { echo "FAIL: $*"; fail=$((fail + 1)); }

# The expression is the single-quoted jq program on the line after `--jq \`.
EXPR="$(grep -F "scan(\"(?:Closes|Fixes|Resolves)" "$SKILL" | head -1 | sed -e "s/^ *'//" -e "s/'\$//")"
if [ -z "$EXPR" ]; then echo "FAIL: no scan expression found in $SKILL"; exit 1; fi

T="$(mktemp -d "${TMPDIR:-/tmp}/scan-test.XXXXXX")" || exit 1
trap 'rm -rf "$T"' EXIT
cat > "$T/fixture.json" <<'EOF'
[
  {"body": "Summary.\n\nCloses #227"},
  {"body": "fixes #234 and closes #5"},
  {"body": "Resolves #12\nRESOLVES #9"},
  {"body": "mentions #77 but claims nothing"},
  {"body": ""}
]
EOF

got="$(jq -c "$EXPR" "$T/fixture.json" 2>&1)"
want='[227,234,5,12,9]'
[ "$got" = "$want" ] || bad "fixture: got $got, want $want"

# A bare digit-per-item bug (the old `[][0]` form) would print [2,2,2,...] here.
got1="$(printf '[{"body":"Closes #1234"}]' | jq -c "$EXPR" 2>&1)"
[ "$got1" = '[1234]' ] || bad "single multi-digit: got $got1, want [1234]"

if [ "$fail" -eq 0 ]; then echo "PASS: claim-scan expression extracts full issue numbers"; exit 0; fi
echo "$fail failure(s)"; exit 1
