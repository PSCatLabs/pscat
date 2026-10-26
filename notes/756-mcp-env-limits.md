# #756: pscat-mcp limits from the environment

A plugin user could not raise `--timeout`, `--max-steps` or `--max-pixels`
without disabling the plugin's server and registering one by hand, because the
manifest (`plugin/mcp.json`, a CODEOWNERS path) passes no `args`. `pscat-mcp`
now reads `PSCAT_MCP_<FLAG>` for every numeric flag instead; the manifest is
untouched.

Design: `NUMERIC_OPTIONS` is one table of flag, variable, range and default.
Flags are parsed first (so `--help` and `--version` work with a bad variable
set), then every variable is read and validated (even one a flag overrides),
then a flag beats a variable beats the default. An empty variable is unset. A
bad value exits 2 naming the variable.

Not added, on purpose: variables for `--allow-read` and `--out-root`. They
widen reads and move writes, and #740 keeps writes flag-only; a settings file
or a project's `.claude/settings.json` must not be able to change them.

How the client delivers the variables (checked October 2026):
- Claude Code copies a settings `env` block into its own environment, and
  stdio MCP servers inherit it, unless `CLAUDE_CODE_MCP_ALLOWLIST_ENV=1`.
- Codex starts stdio servers with a short allowlist (`DEFAULT_ENV_VARS` in
  `codex-rs/rmcp-client/src/utils.rs`: `HOME`, `PATH`, `LANG`, ...) plus the
  server's own `env`/`env_vars`; user config cannot add those to a plugin's
  server. So under Codex the variables do not reach the plugin's server; it
  needs the hand-registered route, or an owner change to the plugin's
  `.mcp.json` (follow-up, owner path).

Tests: unit tests over an injected lookup (no `set_var`), and two process
tests in `tests/mcp.rs` (env picked up and overridden by a flag; bad value
refused, `--help` still works). The `Server` helper removes every
`PSCAT_MCP_*` variable from its child so a developer's shell cannot change a
test.
