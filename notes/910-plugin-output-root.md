# 910: the plugin's render output root, from a desktop workspace

From the Codex desktop trial of plugin 0.7.0-rc.1. The tools advertised the
installed plugin (`~/.codex/plugins/cache/pscat/pscat/<version>/`) as the
output root, so the agent could not save into its task workspace. It decoded
the inline PNG's base64 into a temporary file and then into the workspace.

## Why the root was the plugin

`pscat-mcp`'s default output root is its working directory. Codex starts an
Agent Plugins stdio server with `cwd` defaulting to `${PLUGIN_ROOT}`, and a
`cwd` may only name a path inside `${PLUGIN_ROOT}` or `${PLUGIN_DATA}`
(`codex-rs/codex-mcp/src/agent_plugin_config.rs`,
`normalize_agent_plugin_stdio_server`). Without the read-only bits of a
managed install, `out_path` would have written into the bundle. Claude Code
starts plugin servers in the project. The Claude desktop app sometimes starts
them in `/`, where `out_path` was off.

## What each host lets a plugin user configure (researched 2026-10-29)

- **Claude Code**: `userConfig` in `.claude-plugin/plugin.json` (types include
  `directory`), substituted as `${user_config.KEY}` in a stdio server's
  `command`, `args` and `env`. It is set with `/plugin configure` or `/config`
  and saved under `pluginConfigs` in user or managed settings only, never
  project settings (https://code.claude.com/docs/en/plugins-reference#user-configuration).
  `${CLAUDE_PROJECT_DIR}` is substituted in the same fields
  (https://code.claude.com/docs/en/mcp, "Plugin-provided MCP servers").
  Checked with claude 2.1.312: an unset option with `"default": ""` arrives as
  an empty string, and one set in `pluginConfigs` arrives as the path. 2.1.301
  (CI's pin) validates the manifest with `--strict`. `claude plugin install`
  notes "1 userConfig option not yet set" and does not stall.
- **Cowork / Claude desktop**: "MCP server that references `${user_config.*}`
  values: Ignored when a referenced option has no default; Cowork doesn't
  prompt for values" (https://claude.com/docs/plugins/platform-support). So the
  option has `"default": ""`, and the launcher also treats a literal `${...}`
  as unset.
- **Codex**: the user can set only `enabled`, `default_tools_approval_mode`,
  `enabled_tools`, `disabled_tools` and per-tool approval for a plugin's server
  (`plugins.<plugin>.mcp_servers.<server>.*`,
  https://learn.chatgpt.com/docs/config-file/config-reference). There are no
  args, env or cwd, and no workspace variable: only `PLUGIN_ROOT` and
  `PLUGIN_DATA` are expanded (https://developers.openai.com/codex/plugins/build,
  and the source above). A per-user file is the only setting the user can
  change without replacing the whole registration.
- **MCP roots**: Claude Code answers `roots/list` with the launch directory and
  the `--add-dir` directories. Codex's source has no `roots/list`. The spec
  marks roots deprecated (DRAFT-2026-v1) and calls them informational, not
  access control. Not used here.

## What changed

The launcher (`plugin/scripts/pscat-mcp.sh`) chooses `--out-root` at launch,
unless the arguments already name one. It takes the first usable of:

1. `--plugin-out-root=${user_config.output_dir}` (Claude Code's option);
2. `out-root = DIR` in `${XDG_CONFIG_HOME:-~/.config}/pscat/plugin.conf`;
3. `--plugin-project-dir=${CLAUDE_PROJECT_DIR}`;
4. the working directory (what the server took before);
5. `renders/` in pscat's per-user data directory, created on demand.

A directory is usable only if it is absolute (or `~/...`), exists, and is
neither `/` nor inside the plugin. Both sides are compared after `pwd -P`. A
bad setting is skipped with a warning on stderr. If nothing is usable (no
HOME), the server is started in `/`, which turns `out_path` off. The
`--plugin-*=` arguments are launcher-only and never reach `pscat-mcp`.

**Trust.** Every source is fixed at registration or by the user's own files.
No tool argument moves the root, and no environment variable names it: the
server's `no_env_form_for_reads_or_writes` is unchanged, and the launcher
reads no `PSCAT_MCP_OUT_ROOT`. `pluginConfigs` is user or managed settings
only, and `~/.config/pscat/plugin.conf` is the user's own file. A render
cannot write `plugin.conf` (`out_path` takes only `.png`/`.svg`/`.pdf`), and
PostScript writes nothing (#740). The per-path checks from #627 (traversal,
symlinks, dot-directories, extension, temp-file rename) are untouched, in
`confine` / `Dest::write_pages`.

**Why `~/.config/pscat` on macOS too.** It is a file people edit by hand, so
there is one path to document for both systems (as git and gh do). pscat's
per-user data directory (`~/Library/Application Support/pscat`) holds data
and is a read root for programs. The default `renders/` lives there instead,
outside `~/Documents` and `~/Pictures`, so starting the server never triggers
a macOS privacy prompt.

**Behaviour change.** A desktop host that started the server in `/` had
`out_path` off. It now saves into the default `renders/` directory. Under
Codex without a setting, renders that would have gone into the plugin (or
failed on a read-only install) also go there.

The server's instructions add one sentence: when the user's workspace is
outside the root, write the file there and copy it; don't decode an inline
image. The pscat skill says the same in a line.

## Tests

- `scripts/test_plugin_launcher.py`: a read-only plugin as the cwd gives the
  default directory, and `PSCAT_MCP_OUT_ROOT` is ignored. Covered too: the
  config file (comments, quotes, `~/`, `XDG_CONFIG_HOME`), each unusable value
  with its warning, Claude's bindings and their precedence (empty, an
  unexpanded `${...}`, a project of `/`), an explicit `--out-root` left
  alone, and no HOME (server started in `/`).
- `scripts/codex_plugin_install_check.sh`: the real Codex install made
  read-only and used as the cwd, with real `pscat-mcp` saves of PNG, SVG and
  PDF. With a config file they land in the configured root; without one, in
  `renders/`. No file appears in the copy, and the instructions name the root.
- `scripts/plugin_install_check.sh`: `pluginConfigs` sets `output_dir`, and
  pscat-mcp (seen through a recording `PSCAT_MCP_BIN`) starts with
  `--out-root` that directory.
- `tests/mcp.rs`: the instructions name the root and the copy recipe.

## Not done

- Reads: under Codex the read roots are still the cwd, the plugin. A program
  cannot `run` a `.ps` file from the user's workspace unless it passes the
  source inline. This needs its own decision (#763's trust model).
- A command to write `plugin.conf`; the README gives a two-line recipe.
