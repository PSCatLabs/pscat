## The Claude Code plugin and its marketplace (issue #569 part a, 2026-10-16)

Part (a) of #569 only: a local Claude Code plugin installable from a
marketplace in this repo. The Codex plugin (b), the MCP Registry entry (c)
and directory submissions (d) stay open on #569 for 0.6.x.

**Layout.** `.claude-plugin/marketplace.json` at the repo root lists one
plugin, `pscat`, with `source: "./plugin"`. `plugin/` holds
`.claude-plugin/plugin.json`, a README (the listing text: description,
install, example prompts, privacy note) and `skills`, a symlink to the
top-level `skills/` from #568. The format was checked against the current
docs (code.claude.com plugins manifest and marketplace references) and
`claude plugin validate` in Claude Code 2.1.301, not guessed.

Why a subdirectory and not a repo-root plugin: install copies the plugin
directory into `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/`.
Measured with a throwaway config, a root plugin (`source: "./"`) copied
136 MB (the whole checkout, fonts and gallery included), and
`claude plugin validate` warned that the root `CLAUDE.md` is not loaded.
The `plugin/` layout copies 68 KB. The copy follows the symlink: the
cache holds real `skills/*/SKILL.md` files, no symlink. `claude plugin
validate --strict plugin` warns that it does not follow a symlinked
component directory, so CI validates the marketplace and `skills/`
separately, both with `--strict`. Windows checkouts without symlink
support would get a text file at `plugin/skills`: untested, documented,
filed as #633.

**MCP server.** Declared inline in plugin.json:
`"pscat": {"command": "pscat-mcp"}`. A root `.mcp.json` would also be a
project-scope server for anyone opening the checkout in Claude Code, so it
is not used. No args: the server's default output root is the session's
working directory, which is what an installed plugin wants. The command is
a bare name looked up on PATH; nothing names a repo path.

**Missing binaries.** The plugin requires a separately installed
`pscat`/`pscat-mcp` (cargo or a release archive); no binaries are bundled.
Without them Claude Code reports the server as `✘ Failed to connect —
ENOENT: Executable not found in $PATH`, which is not much of a hint, so a
SessionStart hook runs `command -v pscat && command -v pscat-mcp` and, if
either is missing, prints a `systemMessage` with the install commands.
It is silent when both are found. Installing a plugin with a hook asked for
no confirmation (`claude plugin install` ran non-interactively).

**Versioning.** plugin.json's `version` is the crate version. The
marketplace entry has none (plugin.json wins at install and validate warns
about duplicates). `scripts/plugin_check.py` fails when they differ, so a
release bump that misses `plugin/.claude-plugin/plugin.json` fails the
`plugin` workflow. #602 owns `scripts/release.sh` and bumps the manifest;
`claude plugin tag` can also check the manifest and entry agree.

**What was tested.**
- macOS (local, Claude Code 2.1.301), with a throwaway `CLAUDE_CONFIG_DIR`:
  `plugin marketplace add <checkout>`, `plugin install pscat@pscat`,
  `plugin details` (3 skills, 1 hook, 1 MCP server), and `claude mcp list`
  with pscat from a `cargo install --root <scratch>` outside the checkout
  (connected) and without it (failed). `scripts/plugin_install_check.sh`
  automates this.
- macOS model session: a throwaway config is not logged in, so `claude -p
  --plugin-dir <installed cache copy>` ran against the real login, which
  installs nothing. It listed the skills as `pscat:pscat`, `pscat:psart`,
  `pscat:psviz`, called `mcp__plugin_pscat_pscat__render_postscript` and
  described the blue square it got back. With pscat off PATH the hook's
  message appeared. Checksums of `~/.claude/settings.json` and the
  plugin registries were unchanged afterwards.
- Linux: the `plugin` workflow's `install` job runs the same script on
  ubuntu-latest with a pinned `@anthropic-ai/claude-code`, from a fresh
  runner. A model session in CI needs a login and is not run.
- From GitHub, against the pushed branch, each in a fresh throwaway config
  on macOS: `marketplace add jeromebanks/postscript_interpreter#<branch>` +
  `install pscat@pscat` (marketplace clone 248 MB, cache copy 76 KB with
  real SKILL.md files, `mcp list` connected); the same with `--sparse
  .claude-plugin plugin skills` (clone 704 KB, symlink still resolved, 3
  skills installed); and the one-line `plugin install pscat --marketplace
  jeromebanks/postscript_interpreter#<branch>`.

**Gotcha.** Claude Code caches a failed MCP connection for about 15
minutes, keyed to the server config. After testing the missing-binary case,
a session with pscat back on PATH still showed the server as failed until
the config changed.
