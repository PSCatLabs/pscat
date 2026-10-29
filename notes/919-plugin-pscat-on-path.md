# 919 and 920: pscat on PATH after the plugin installs; one stable binary path

Both came from the v0.7.0-rc.1 trial of the plugin in a claude.ai cloud
session (Linux x86_64), and both touch the launcher
(`plugin/scripts/pscat-mcp.sh`), the session-start hook and the pscat skill,
so they ship together. The Codex plugin has no launcher of its own:
`.agents/plugins/marketplace.json` points at the same `./plugin`.

## What the trial showed

- After install `pscat --help` failed. The agent ran `--locate`, which
  downloaded the release, then used the versioned path
  (`~/.cache/pscat/plugin/v0.7.0-rc.1/x86_64-unknown-linux-musl/pscat`) on
  every call.
- The hook ran (the session-start note appeared) but the `pscat-mcp` tools
  never did, so nothing had downloaded pscat: `--locate` found an empty cache.
  That is why linking `$CACHE/bin/pscat` alone is not enough on a fresh
  install: the link would dangle.
- The docs named three different locations: the skill said `.../bin/pscat`,
  `--locate` and the hook said the versioned directory, and the first hook
  note pointed at the synced plugin directory for the launcher.
- The hook and README said the download is "about 6 MB". The rc.1 archives are
  14.2 to 14.4 MB (v0.6.3 was 6 MB). Now "about 14 MB".

## What changed

- **One stable path: `$CACHE/bin/pscat` and `pscat-mcp`.** The launcher's
  `lookup` now re-points `bin/` at the release it just validated (it was only
  written at install time, so after an older pin, or two hosts pinning
  different releases, it could name another version). `--locate` prints
  `$CACHE/bin` (one line, stdout) when the cached copy is in use; the hook and
  the skill say `$CACHE/bin/pscat`. The versioned directory is no longer
  printed anywhere. The offline fallback uses `bin/` too. The launcher still
  `exec`s the real file in the versioned directory.
- **PATH links.** `link_on_path` runs in `--locate`, in the hook (cached copy
  found) and on a normal server start. It links `pscat` and `pscat-mcp` into
  the first of `~/.local/bin`, `~/bin` that exists, is writable and is already
  on `PATH`. Rules:
  - Never a system directory (`/usr/local/bin` is Homebrew's on an Intel Mac,
    and root can write there in a cloud session), never `~/.cargo/bin` (cargo
    refuses to install over a file it did not make), never `mkdir`.
  - "Ours" is a symlink whose target is exactly `$CACHE/bin/<name>`, or the
    marked shim (line 2 `# pscat-plugin-shim`, `pscat` only). Anything else
    named `pscat` there (file, other symlink, dangling link, directory) is left
    untouched. If `command -v pscat` already finds a pscat that is not ours,
    nothing is linked at all (we cannot know which would win).
  - New names are made with one `ln -sn` (fails if the name appeared
    meanwhile); our own old link or shim is replaced with tmp plus `mv -f`.
  - `pscat-mcp` is only ever a symlink, never a shim: the launcher's own PATH
    lookup runs `pscat-mcp --version` and a shim there would hit the recursion
    guard.
  - `PSCAT_NO_PATH_LINK=1` turns it off.
- **The shim, for a fresh install.** With no cached copy the hook (which still
  never downloads, #853) leaves `~/.local/bin/pscat` as a ten-line shell script
  that runs the launcher's `--locate` (download messages on stderr), then execs
  `$CACHE/bin/pscat`. The launcher's `link_on_path` replaces the shim with the
  symlink as soon as the cache exists, so the shim only exists before the first
  download. The shim names the launcher by absolute path and the hook rewrites
  it when the plugin directory changes (a version bump or sync moves it).
  If the plugin is gone and the cache is empty it prints a clear message and
  exits 127. This is what makes "`pscat --help` works in the first shell
  command" true on a fresh install.
- **Hook text.** The not-installed message gives the size (about 14 MB, once)
  and the stable path that will exist afterwards, and keeps the phrase
  "downloads it" that `plugin_install_check.sh` and
  `codex_plugin_install_check.sh` assert.
- **Skill.** The install paragraph of `plugin/skills/pscat/SKILL.md` (the
  top-level `skills` is a symlink to it) leads with `pscat --help`, says the
  plugin puts pscat on PATH, gives the one stable path per OS, and has one line
  that the first use downloads about 14 MB.
- README, HANDOFF and `docs/guides/agents.md`: the same facts; the README
  covers how to undo the links and that this is the one thing the plugin writes
  outside its cache.

## pscat-mcp in claude.ai cloud sessions

What the docs say (checked 2026-10-28):
- `claude.com/docs/plugins/platform-support`: "Local MCP server, a command the
  app starts": Chat "Ignored", Cowork "Loads when the Cowork session runs on
  your computer", Claude Code "Loads". Hooks: Chat ignored, Cowork and Code
  load. `bin/`: cannot be installed in Chat or Cowork.
- `code.claude.com/docs/en/plugins/loading`, "Plugins synced from claude.ai":
  in terminal sessions a synced plugin's skills, agents, hooks and MCP servers
  all load. In Cowork the plugin is downloaded into the session's own
  environment. The trial's `/root/.claude/plugins/synced/<id>/` path is that
  case.
- `code.claude.com/docs/en/cloud-environments`, "What carries over": a repo's
  own `.mcp.json` servers load in a one-repo cloud session; plugins and
  marketplaces declared in a repo's `.claude/settings.json` do not install
  there (no workspace trust dialog); user-scope plugins do not carry over.
  The Trusted network level includes `github.com` and
  `release-assets.githubusercontent.com`, so the launcher's download can work.

So the trial (hook and skills loaded, no MCP server) matches the documented
Cowork rule for a session that does not run on the user's computer. The
docs do not say that a *synced* plugin's local server starts, or does not, in
a Claude Code on the web session specifically. The plugin cannot register the
server itself there; nothing in a plugin's files can force it. The only
documented ways a pscat tool reaches a cloud session are a committed
`.mcp.json` in the user's repo (one-repo sessions) and a remote `http`
server, which is the one MCP kind listed as working in chat, Cowork and Code.
That is the hosted MCP plan (epic #689). A definitive answer for the current
plugin needs `/mcp` run in a live cloud session with the plugin installed.

## Not verified here

- A real cloud session: only the launcher against a file:// fake release, the
  `claude` CLI install check, and the Codex install check.
- Whether the hook's PATH matches the Bash tool's PATH in a cloud session. In
  the trial `~/.local/bin` was on the Bash tool's PATH. If the hook's PATH lacks
  it, nothing is linked and the hook text names `$CACHE/bin/pscat` instead.
