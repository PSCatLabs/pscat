# 853: the plugin installs pscat itself (pscat-mcp launcher)

Part of #682. The aim: a non-developer on the Claude desktop app (Cowork) adds
the `PSCatLabs/pscat` marketplace, installs the plugin, and the pscat tools
work without Rust, Node or a manual install. Before this, both manifests ran
the bare command `pscat-mcp`, which fails unless pscat is installed *and* the
GUI app's PATH includes `~/.cargo/bin`.

## What the hosts expand (October 2026)

| Host | File | Expands | Where | Exported to the server |
|---|---|---|---|---|
| Claude Code | `.claude-plugin/plugin.json` `mcpServers`, `.mcp.json` | `${CLAUDE_PLUGIN_ROOT}`, `${CLAUDE_PLUGIN_DATA}` (`~/.claude/plugins/data/<id>/`), `${user_config.*}` | `command`, `args`, `env` | `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA` |
| Claude Code | hook commands | the same plus `${CLAUDE_PROJECT_DIR}` | `command`, `args` | also `CLAUDE_PROJECT_DIR` |
| Cowork / claude.ai | `.claude-plugin/plugin.json` | `${CLAUDE_PLUGIN_ROOT}` (the org-sync page says to reference executables as `${CLAUDE_PLUGIN_ROOT}/scripts/<name>` from "skills, hooks, or MCP server configs"); `CLAUDE_PLUGIN_DATA` is not documented for Cowork | | not documented |
| Codex 0.164 | `mcp.json` (agent-plugins format) | `${PLUGIN_ROOT}`, `${PLUGIN_DATA}` only, not the `CLAUDE_` spelling | `args`, `env`, `cwd`; **not** `command`, which must be bare or a contained `./` path | `PLUGIN_ROOT`, `PLUGIN_DATA`; `cwd` defaults to the plugin root |
| Codex | hooks | `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA` "for compatibility", plus `PLUGIN_DATA` | | |

Sources:
- Claude Code: <https://code.claude.com/docs/en/plugins-reference>, "Environment variables" and "Where each variable resolves". The Standard layout table says of `bin/`: "claude.ai and Cowork don't install a plugin that has this directory".
- Cowork: <https://claude.com/docs/cowork/guide/plugins> (install, marketplaces).
- <https://claude.com/docs/plugins/platform-support>: "Local MCP server … Loads when the Cowork session runs on your computer", and the `bin/` row: "Can't be installed".
- <https://claude.com/docs/plugins/org-sync>: "Keep executables out of the top-level bin directory".
- Codex: `codex-rs/codex-mcp/src/agent_plugin_config.rs` (`normalize_agent_plugin_stdio_server`, `expand_agent_plugin_placeholders`), the spec at agent-plugins.org ("Placeholder expansion does not apply to `command`"), and <https://learn.chatgpt.com/docs/hooks>.
- Checked by running each host: `scripts/plugin_install_check.sh` (claude 2.1.310) connects through `sh ${CLAUDE_PLUGIN_ROOT}/scripts/pscat-mcp.sh`. Locally, `codex mcp get pscat --json` (codex 0.164) shows `args` expanded to the installed launcher, with `PLUGIN_ROOT`/`PLUGIN_DATA` in `env`. Codex's `PLUGIN_DATA` for the server is `plugins/data/agent-plugins/<hash>`.

So the issue's example path `plugin/bin/pscat-mcp` would have made Cowork
refuse the whole plugin. The launcher is `plugin/scripts/pscat-mcp.sh`, and
`plugin_check.py` now fails on a top-level `bin/`.

## What changed

- `plugin/scripts/pscat-mcp.sh` (POSIX sh; runs under dash, macOS sh and
  `bash --posix`):
  - **Lookup order:** `$PSCAT_MCP_BIN`, PATH, `~/.cargo/bin`,
    `/opt/homebrew/bin`, `/usr/local/bin`. A candidate counts only if
    `pscat-mcp --version` is at least the pin's X.Y.Z. It is run with stdin
    at EOF, so an old binary without `--version` exits.
  - **Then the cache:** the cached copy of the pinned release.
  - **Otherwise download:** fetch `SHA256SUMS` and the one archive whose
    name ends `-<triple>.tar.gz`. The match is by suffix, so an rc's
    archives can carry `0.7.0-rc.1` or `0.7.0`. The script checks the sha256
    and unpacks the archive, then execs the bundle's own `pscat-mcp` so it
    finds `lib/` beside itself. It needs only curl or wget, tar and
    sha256sum, shasum or openssl.
  - **Offline:** a clear stderr message, and exit 1. If a previous pin's
    download is cached, it uses that instead and warns. An HTTP error or a
    missing file (curl 22/37, wget 8) is reported as "is the release
    published (not a draft), and is the tag right?" instead, so a typo or
    a draft rc does not send anyone to check their Wi-Fi.
- **Concurrency.** Each installer unpacks into its own `mktemp` directory
  inside the cache. It publishes with `ln -sn <its dir> <tag>/<triple>`,
  which fails if the name exists (`-n` matters: without it `ln` follows an
  existing link to a directory and succeeds inside it), so exactly one racer
  wins and a reader
  never sees a half-unpacked tree. The losers delete their own copies. A
  `mkdir` lock with a pid file (a stale lock is cleared when its process is
  gone) only stops eight parallel starts from downloading eight times.
- **Cache location.** `~/Library/Caches/pscat/plugin` on macOS, or
  `${XDG_CACHE_HOME:-~/.cache}/pscat/plugin`; `PSCAT_CACHE_DIR` overrides.
  The issue suggested `${CLAUDE_PLUGIN_DATA}` first. It isn't used, because
  it differs per host:
  - Codex gives the server `PLUGIN_DATA` but gives hooks `CLAUDE_PLUGIN_DATA`.
  - Cowork doesn't document either.
  - The Bash tool sees neither.
  One OS cache dir means one download serves Claude Code, Cowork and Codex.
  It also lets the hook and the skills name a path that exists, and
  `bin/pscat` there is a stable name. The cost is that uninstalling the
  plugin doesn't delete it; the README says how.
- **stdout** carries only what `pscat-mcp` prints (the MCP channel),
  `--locate`'s directory, or `--hook`'s JSON. Everything else goes to stderr.
- **`plugin/pscat-release`**: the tag, one line, with `#` comments allowed.
  `plugin_check.py` requires it to equal `v<crate version>`, or to be a
  pre-release of a version not older than the crate, which is the rc trial
  case.
- **Hook:** `hooks.json` runs the launcher with `--hook`, and
  `hooks/check-pscat.sh` is gone. The hook never downloads.
  - With pscat on PATH it prints nothing, as before.
  - With pscat found elsewhere or cached, it sends `additionalContext`
    naming `<dir>/pscat` for Claude.
  - With nothing found, it sends a `systemMessage` to the user saying the
    server downloads it, and `additionalContext` with the `--locate`
    command as an absolute path. The Bash tool has no plugin variables, so
    the path has to be spelled out.
- **Manifests:** Claude's `plugin.json` has `"command": "sh", "args":
  ["${CLAUDE_PLUGIN_ROOT}/scripts/pscat-mcp.sh"]`. Codex's `mcp.json` has
  the same with `${PLUGIN_ROOT}`. The `sh` prefix means a copy that lost its
  exec bit still runs, and it keeps placeholders out of Codex's `command`.
  The descriptions no longer say "Requires pscat installed separately".
- **`plugin_check.py`** checks four new things:
  - each host's placeholder in each host's file, and no placeholder in a
    Codex `command`;
  - that a `${...PLUGIN_ROOT}/<file>` in server args exists;
  - that a server both manifests declare runs the same command and args;
  - no top-level `bin/`, and the pin rule above.
- **Tests:**
  - `scripts/test_plugin_launcher.py` builds a fake `file://` release (a
    fake `pscat-mcp` that prints a marker) and covers 12 cases, under sh,
    dash and `bash --posix`: cold and warm start with stdout exactly the
    marker, `--locate`, eight parallel cold starts leaving one install and
    no lock, checksum mismatch installing nothing, offline, the offline
    fallback, an installed new-enough copy used (PATH, `~/.cargo/bin`,
    `PSCAT_MCP_BIN`), an old copy passed over, a stale lock, the hook's
    three outputs, a missing release told apart from offline, and a
    release-version pin. Mutating the launcher (stdout noise, skipping the
    checksum) fails it; dropping the lock fails only the lock tests, the
    parallel test still passing on the `ln -sn` publish alone.
  - Both install checks now prove three cases through the real hosts' own
    resolution: on PATH; not on PATH, so a download from
    `scripts/fake_plugin_release.py` (the real binaries); and offline,
    which fails. None of them use the network.
- `.github/CODEOWNERS`: `/plugin/scripts/` and `/plugin/pscat-release` are
  owner paths, since they decide what binary runs on users' machines.

## Directory portal

The server script and the hook now use shell variables and `$(...)`. The
pre-submission checklist holds those for a reviewer when the plugin is a
subfolder of the repo: "Keep … shell variables other than
`${CLAUDE_PLUGIN_ROOT}`, command substitutions, and calls to other files in
the plugin out of those scripts … Held for a reviewer", under the title
"Scripts the validator couldn't follow". That reverses #684's
followable hook. The issue already expected a "held for reviewer" note about
the MCP command. The README's Privacy section discloses the one network
fetch, for the security scan.

## Timing

A real `--locate` against v0.6.3 from GitHub (6 MB, macOS arm64) took
1.4 s, and a warm start makes no request. On a slow link the first start
may outlast the client's connect timeout. The README says to restart; an
unfinished download is discarded and redone on the next start.

## Not done / follow-ups

- **Windows.** The server entry runs `sh`, so Windows users who had
  `pscat-mcp` on PATH lose the plugin's server unless Git for Windows puts
  `sh` on PATH. The launcher's lookup works under Git Bash, but it downloads
  nothing there. The README tells them to register `pscat-mcp` by hand. A
  PowerShell twin can't be selected per OS from one manifest entry.
- **Done in #867:** `bump-manifests` now sets `plugin/pscat-release`
  (`notes/867-bump-manifests-pin.md`). What follows is the original note.
- **`release_stages.py bump-manifests`** (used by `scripts/release.sh`;
  #854's lane, owner files) should bump `plugin/pscat-release` with the
  other manifests. #854 (PR #856) puts rc versions in Cargo.toml as
  `0.7.0-rc.1` with tag `v0.7.0-rc.1`; the pin rule accepts that pair. Until it does, a release
  bump PR fails `plugin_check` on the pin (a release pin must equal the
  crate version), which is the reminder. An rc pin of the version being
  released still passes, so after a trial set the pin back by hand.
- Old pins' downloads stay in the cache; nothing prunes them.
- Not run inside a live Cowork session: the owner's trial with the rc pin
  is that test.
