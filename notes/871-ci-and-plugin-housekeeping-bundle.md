# CI and plugin housekeeping bundle (#871, #705, #800, #1030)

One PR for four small tickets.

- **#871** `site-check.yml`: `scripts/site_chalk.py` and `site/chalk/**` added to
  the `paths:` filter. `site/**` already matches `site/chalk/**` in GitHub's glob
  rules, so the second line is explicit rather than new coverage; the script line
  is the real gap.
- **#705** `dependabot.yml`: a second cargo entry for `/fuzz` (its own workspace
  and lockfile), lockfile-only like the root entry.
- **#800** `env_vars` in `plugin/mcp.json` is **not** done, because it cannot
  work. Codex parses `mcp.json` as an Agent Plugins file: the stdio server struct
  in `codex-rs/codex-mcp/src/agent_plugin_config.rs` is `deny_unknown_fields` and
  takes only `type`, `command`, `args`, `env`, `cwd` (the published schema has
  `additionalProperties: false` too). Checked against the real CLI (codex-cli
  0.167.2, a throwaway `CODEX_HOME`, app-server `plugin/read`): with `env_vars` the
  plugin lists no MCP server at all; without it, `pscat`. `env_vars` is a
  `config.toml` key, so Codex users still register the server by hand (see
  `plugin/README.md`). What shipped instead: `scripts/plugin_check.py` rejects any
  extra key on a Codex stdio server so nobody re-adds it, and the tests that start
  `pscat-mcp` directly strip `PSCAT_MCP_*` from the child environment
  (`common::without_mcp_env`).
- **#1030** item 2 (`.claude/skills/functional-postscript` symlink plus a test that
  `.claude/skills/` mirrors `plugin/skills/`) and item 3 (`licenses/assets.json`
  entry and `THIRD_PARTY_NOTICES.md` row for the AI-generated hybrid images). Item 1
  (re-measure `docs/functional-postscript/MEASUREMENTS.json`) left to PR #960,
  which rewrites that file and the files it hashes (`lib/functional.ps`,
  `examples/functional/common.ps`); measuring now would be stale on merge.
