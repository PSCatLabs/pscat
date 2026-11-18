# #1281: every worktree builds into PSCAT_TARGET_ROOT

Only work-issue step 3 linked `target/` to the external drive (#1033). Worktrees made by the
resume route, identity-check base trees and the dispatcher's hand-made integration trees
kept a build on the main drive (one train tree reached 11 GB), and the won't-change cleanup
left the external folder behind.

`scripts/worktree_target.sh link|unlink` is now the one place that knows the rule.
`link` is a no-op without `PSCAT_TARGET_ROOT` (unset or not a folder) and when `target`
exists. `unlink` deletes only the folder the link names, and only when the link text is
exactly `$PSCAT_TARGET_ROOT/<one name>` (trailing slashes on the root are ignored; `..`,
nested, relative and outside links are left alone). Used by step 3, the resume route, the
identity base instructions, both finish.md cleanups and EXTRA_WORKTREES removal; CLAUDE.md
states the rule for hand-made worktrees. Cleanup unlinks run before `git worktree remove`,
and use the copy of the helper inside the merged worktree, because the main checkout can be
stale and a detached base tree may predate the script.

Test: `scripts/test_worktree_target.sh` (run under `/bin/bash`, in `links.yml`); every case
uses its own temporary root.
