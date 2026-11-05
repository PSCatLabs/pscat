# 1012: plugin manifests said "Three skills"

`plugin/plugin.json`, `plugin/.claude-plugin/plugin.json` and
`.claude-plugin/marketplace.json` described the plugin as "Three skills plus
the pscat-mcp server" after #923 added `functional-postscript` as the fourth.

- The three descriptions now say "Agent skills plus the pscat-mcp server":
  no count to go stale. `plugin/README.md` says "Four skills" and lists all
  four; the README.md and HANDOFF.md skill lists name `functional-postscript`.
- `tests/skill_blocks.rs` `manifests_and_listings_state_the_real_skill_count`
  fails if any manifest, marketplace file, `server.json` or the two READMEs
  state a skill count other than the number of `plugin/skills/*/SKILL.md`.
  Wording without a count passes. A unit test covers the scanner (number words,
  digits, Markdown emphasis).
- `scripts/plugin_install_check.sh` and `scripts/codex_plugin_install_check.sh`
  looped over `pscat psart psviz`, so neither checked that
  `functional-postscript` installs. They now loop over `plugin/skills/`.
- Not done: `.claude/skills/` still symlinks only the first three plugin
  skills for checkout sessions.
