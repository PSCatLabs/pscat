# #569 part (b): the Codex local plugin

Part (a) (`notes/569-claude-code-plugin.md`) shipped the Claude Code plugin
in `plugin/`. This adds the Codex equivalent from the same directory and
the same skill text.

## Format, from OpenAI's docs (checked 2026-10-19)

- <https://developers.openai.com/plugins/build/plugins>: a portable plugin
  has `plugin.json` at its root (schema
  `https://agent-plugins.org/schemas/1.0.0/plugin.schema.json`), skills
  discovered from `skills/`, MCP servers in a root `mcp.json` whose entries
  need a transport `type`. `.codex-plugin/plugin.json` is an optional
  compatibility overlay. A repo marketplace lives at
  `.agents/plugins/marketplace.json`; entries need `policy.installation`,
  `policy.authentication` and `category`; a local `source.path` starts with
  `./`, resolves from the marketplace root (the repo root, not
  `.agents/plugins/`) and may be any subdirectory. Hooks default to
  `hooks/hooks.json`. `$REPO_ROOT/.claude-plugin/marketplace.json` is read
  as a legacy-compatible location too.
- <https://developers.openai.com/plugins/deploy/desktop-only>: the stdio
  entry shape (`"type": "stdio"`, `command`, `args`, `cwd`, with
  `${PLUGIN_ROOT}`). It says to bundle the server; the resolved spec chose
  a separately installed pscat instead (bundling is part d).
- <https://learn.chatgpt.com/docs/hooks> (formerly
  developers.openai.com/codex/hooks): hooks need user trust (`/hooks`);
  SessionStart JSON `systemMessage` shows as a warning; plugin hook
  commands get `PLUGIN_ROOT` and also `CLAUDE_PLUGIN_ROOT`.

The issue text says "root `plugin.json`". That matches the docs, with
"root" meaning the plugin root (`plugin/`), not the repo root.

## Layout

- `.agents/plugins/marketplace.json`: marketplace `pscat`, one local entry
  at `./plugin`, `AVAILABLE` / `ON_INSTALL`, category `Design`.
- `plugin/plugin.json` (Codex) beside `plugin/.claude-plugin/plugin.json`
  (Claude Code); `plugin/mcp.json` (Codex only; Claude declares the server
  inline); `plugin/hooks/hooks.json` (both).
- `plugin/skills/` holds the skills as real files. Top-level `skills` and
  `.claude/skills/{pscat,psart,psviz}` are symlinks into it.

## What was measured

- **Codex skips symlinks when it copies a plugin into its cache.** With
  codex-cli 0.163.0 and a throwaway `CODEX_HOME`: `plugin/skills ->
  ../skills` gave a cached plugin with `plugin.json` and `mcp.json` and no
  `skills/`; per-skill symlinks inside a real `skills/` were dropped too;
  only the real skill directory was copied. A `codex exec` session listed
  only the real skill. That forced the move into `plugin/skills/`: the
  alternatives were a repo-root plugin (136 MB per install) or a generated
  copy of the skill text (ruled out by the spec).
- With the layout above, `codex plugin marketplace add <checkout>` +
  `codex plugin add pscat@pscat` (no login) installs 88 KB under
  `plugins/cache/pscat/pscat/0.6.0/`, the version from `plugin/plugin.json`.
- A logged-in `codex exec` session with `pscat-mcp` on PATH listed the
  skills as `pscat:pscat`, `pscat:psart`, `pscat:psviz` and the server's four
  tools. The tool call itself was refused only because `codex exec` runs
  with approvals off ("MCP tool call requires approval, but approval
  policy is never"), so the protocol round trip is checked by the install
  script instead.
- The bare prototype manifest (name, version, description only) installed
  as `.../local/` with a `.codex-plugin/plugin.json` overlay Codex wrote
  from the marketplace category; the real manifest installs under `0.6.0/`
  with no overlay. The cause was not pinned down; the install script
  asserts the versioned path.
- Claude Code (2.1.303) picks up `hooks/hooks.json` by default
  (`plugin details`: Hooks (1)) and ignores `plugin.json`/`mcp.json`
  (MCP servers (1)); `scripts/plugin_install_check.sh` still passes.
- The Codex hook could not be run end to end here: hooks need a trust
  review in `/hooks`, which a non-interactive session cannot give.

## Checks

- `scripts/plugin_check.py` now checks the Codex marketplace and plugin too,
  and rejects any symlink inside a plugin directory.
- `scripts/codex_plugin_install_check.sh` (workflow `plugin`, job
  `codex-install`, codex CLI pinned to 0.163.0): install into a throwaway
  home, check the cached files, then start the server from the installed
  `mcp.json` and expect `render_postscript` in `tools/list`.
- `scripts/release_stages.py plugin_manifests()` returns the Codex
  marketplace and `plugin/plugin.json`, so `bump-manifests` and the
  `plugin` stage cover them.

## #633 (Windows symlinks)

The plugin directory has no symlinks now, so both plugins install the same
on a default Windows checkout. What #633 still covers: the top-level
`skills` and `.claude/skills/*` symlinks (contributor checkouts only) and
the POSIX `sh` hook command.
