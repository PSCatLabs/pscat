#!/usr/bin/env bash
# Summarise which Ghostscript parity tests really ran (issue #985).
#
# Usage: scripts/gs_parity_summary.sh <log> <label>
#
# <log> is the file tests/common's `gs_available()` appends to when
# PSCAT_GS_LOG names it: one `ran <suite>::<test>` or `skipped <suite>::<test>`
# line per probe (the suite keeps same-named tests in different binaries apart).
# Prints a Markdown block (for $GITHUB_STEP_SUMMARY) with the number of
# distinct tests that ran gs and the number that skipped. Never fails the
# job: the enforcement is PSCAT_REQUIRE_GS (a skip panics), this is
# visibility. A missing or empty log means no test probed gs.
set -u
log="${1:?usage: gs_parity_summary.sh <log> <label>}"
label="${2:-tests}"
ran=0 skipped=0
if [ -s "$log" ]; then
  ran=$(awk '$1 == "ran" {print $2}' "$log" | sort -u | wc -l | tr -d ' ')
  skipped=$(awk '$1 == "skipped" {print $2}' "$log" | sort -u | wc -l | tr -d ' ')
fi
echo "### Ghostscript parity tests ($label)"
echo
echo "- ran against a real gs: **$ran**"
echo "- skipped (gs not installed): **$skipped**"
if [ "$ran" -eq 0 ] && [ "$skipped" -eq 0 ]; then
  echo "- (no test probed gs; the log is missing or empty)"
fi
