## work-issue: open-PR claim scan extracts full issue numbers (issue #293, 2026-10-13)

The open-PR scan in `SKILL.md` step 1 was `scan(...)[][0] | tonumber`. `scan` with a capture
group yields one array per match (`["227"]`), so `[]` unwrapped it to the string `"227"` and
`[0]` took its first character: `[2,2,2,2]` on live data. Fixed to
`scan("(?:Closes|Fixes|Resolves) #([0-9]+)"; "gi") | .[0] | tonumber`, which also matches
`closes`/`fixes` in any case, as GitHub does. Live output is `[227,234,228,236]`.

- **Other copies:** none in `references/` or `scripts/` (grep). `scripts/issue_summary.sh`
  line 77 already uses `[scan(...; "gi")]` then `map(.[0] | tonumber)`; it was tested live and
  extracts full numbers, so it is left unchanged.
- **`scripts/test-scan.sh`** reads the expression from `SKILL.md` (so it tests the copy the skill
  runs), applies it to a fixture with multi-digit and mixed-case links, and asserts the output.
  It makes no network call. Against the pre-fix `SKILL.md` it fails.
- **Two jq engines:** `gh --jq` uses gojq, which is embedded in gh. `test-scan.sh` runs system
  jq (1.8.1). The `gi` expression was checked under gojq with
  `gh api repos/jeromebanks/postscript_interpreter --jq '"fixes #1234\nRESOLVES #56\nCloses #7" | [scan(...; "gi") | .[0] | tonumber]'`,
  which printed `[1234,56,7]`.
- **`issue_summary.sh`:** the `gi` scan there was run live (`bash scripts/issue_summary.sh`); its
  In Review rows show PRs for #227, #234, #228 and #236.
- **Impact:** resume and conflict detection via open PRs was silently broken for issue numbers
  of 10 or more. Live locks still prevented double claims.
