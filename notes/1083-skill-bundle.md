# 1083 (with 1037, 1082, 1084): skill and doc notes bundle

One PR, four small issues, all in the published skills and the agents guide.

- **#1037** psart skill: `mix3` and `shade` return `r g b` (three numbers),
  not an array. Source: `lib/artkit.ps` `/mix3`, `/shade`.
- **#1082** `board-math.md`: the `mkalign` line step is `/Leading` x `/Size`
  (default 1.9), raised to the previous line's depth plus this line's height
  plus 0.35 `/Size` for a tall line, plus an `/Irregular` wobble (checked
  against `mkalignbody` in `lib/mathkit.ps` and with `/Irregular 0`: 32.3 pt at
  `/Size 17`). `/Ink` is called after each line's path is appended with the
  line's 0-based index on the stack. A runnable block (it runs in the
  `skill_blocks` test) shows stacking a second block under the first from the
  last record. `pscat chalk --print-ps` is named as the way to read a complete
  board.
- **#1083, #1084** pscat skill: a "No MCP, or a cloud sandbox" section (use
  `--locate` once, then the CLI, `pscat --version` to confirm the release,
  write and send the PNG) and an "In the browser or Node" section (npm
  package, playground, `docs/guides/browser.md`, npm lagging the CLI during a
  release candidate). The claude.ai cloud workspace is a fourth host in "Show
  the result to the user". The "MCP preview is for the model only" sentence is
  PR #1078's (#1055); this bundle links to that section rather than repeat it.
- `docs/guides/agents.md`: "installs the three skills" is now four, naming
  functional-postscript. Line 63 ("symlinks to the three user-facing skills")
  stays: `.claude/skills/` really holds three symlinks.

Not done: `docs/dev/HANDOFF.md` is untouched (PRs #1078 and #1070 both edit it).
No size budget for a SKILL.md exists in `scripts/plugin_check.py` or the
tests; `pscat/SKILL.md` grows by about 30 lines here.
