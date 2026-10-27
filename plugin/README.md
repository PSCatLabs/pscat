# pscat plugin for Claude Code and Codex

Render, preview and debug PostScript, make generative art, and draw
paper-ready charts from Claude Code or Codex, with the
[pscat](https://github.com/PSCatLabs/pscat) PostScript
interpreter. The plugin bundles:

- **Three skills**: `pscat` (the tool reference: render to PNG/SVG/PDF,
  debug, lint, seed sweeps, handwriting-style notes, typed letters and memos,
  photo-to-engraving), `psart` (the render-look-refine loop for generative
  art, painting, dry media, chalkboard maths and style packs) and `psviz`
  (function plots and bar/line/area/scatter/pie charts for papers).
- **The `pscat-mcp` server**, which gives the agent ten tools:
  `render_postscript` (pages come back as images the agent can look at),
  `eval_postscript`, `handwrite`, `chalkboard_math` (LaTeX in, a chalkboard
  PNG back), `review_sheet` (several programs at several sizes, labelled, on
  one PNG), `describe_art_capabilities`, and `start_render`,
  `render_status`, `render_result` and `cancel_render` for a render longer
  than one call allows.

pscat's handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's.

## Requirements: none on macOS, Linux and Windows

You don't need Rust, Node or a separate pscat install. The plugin ships no
binaries; its MCP server entry runs a small launcher, `scripts/pscat-mcp`.
On macOS and Linux that is a POSIX `sh` script, which hands over to
`scripts/pscat-mcp.sh`. On Windows the agent runs `scripts/pscat-mcp.cmd`
beside it instead, and that runs `scripts/pscat-mcp.ps1` in Windows
PowerShell, which every Windows 10 and 11 has (see [Platforms](#platforms)).
Either way, the launcher:

1. **Uses pscat if you already have it.** It looks for `pscat-mcp` in
   `$PSCAT_MCP_BIN` (if set), on your `PATH`, then in `~/.cargo/bin`,
   `/opt/homebrew/bin` and `/usr/local/bin` (on Windows, the Cargo `bin`
   directory), because an app started from the Dock or Start menu often has
   a shorter `PATH` than your terminal. A copy counts only
   if `pscat-mcp --version` is at least the version the plugin pins.
2. **Otherwise downloads it once.** It fetches the archive for your platform
   (macOS arm64 or x86_64, Linux x86_64 or aarch64, Windows x86_64, which
   Windows on ARM also runs) from the GitHub release
   named in `pscat-release`, about 6 MB, from
   <https://github.com/PSCatLabs/pscat/releases>. It checks the archive
   against that release's `SHA256SUMS` and refuses it on a mismatch. Then
   it unpacks it into a per-user cache and runs it from there. It needs only
   `curl` or `wget`, `tar` and `shasum` or `sha256sum`, all of which macOS and
   most Linux systems have; on Windows, only PowerShell.
3. **Starts the server.** Later sessions use the cached copy and make no
   network request.

The cache is `~/Library/Caches/pscat/plugin` on macOS,
`~/.cache/pscat/plugin` on Linux (or `$XDG_CACHE_HOME/pscat/plugin`) and
`%LOCALAPPDATA%\pscat\plugin` on Windows; set `PSCAT_CACHE_DIR` to move it.
The `pscat` command-line tool is there too, at `bin/pscat` (on Windows, in
`<release>\x86_64-pc-windows-msvc\`). Delete the directory to remove the
download.

The first start downloads before the server answers, which takes a few
seconds on a normal connection. If your agent gives up first and lists
the server as failed, restart it, and the second start uses the
finished download. If the computer is offline the first time, the server
fails with a message saying so (Claude Code's `/mcp` shows
`plugin:pscat:pscat` as failed). Connect and restart, or install pscat
yourself:

```sh
cargo install pscat
```

or put the `pscat` and `pscat-mcp` from a release archive on your
`PATH`. A SessionStart hook (`hooks/hooks.json`, which runs the same launcher
with `--hook`; both Claude Code and Codex read it) never downloads. When pscat
isn't on your `PATH` it tells you whether the server will download it, and
tells Claude the full path of the `pscat` command-line tool.

## Install in the Claude desktop app (Cowork)

1. Open **Customize > Plugins**, select **Add > Add marketplace**, and
   enter `PSCatLabs/pscat`.
2. Find **pscat** in the list and select **Install**.
3. Start a new Cowork task and ask for a render, for example "Render a
   PostScript spiral and show me the result."

Cowork starts the plugin's MCP server on your computer, so the launcher
above downloads pscat on the first task (a few seconds). The plugin's
skills also work in chat, but chat doesn't start local MCP servers
(<https://claude.com/docs/plugins/platform-support>).

## Install in Claude Code

In Claude Code:

```text
/plugin marketplace add PSCatLabs/pscat
/plugin install pscat@pscat
```

On Claude Code 2.1.275 or later, one line does both:

```text
/plugin install pscat --marketplace PSCatLabs/pscat
```

From a shell, the same thing is
`claude plugin marketplace add PSCatLabs/pscat` and
then `claude plugin install pscat@pscat`. Restart Claude Code (or run
`/reload-plugins`) to load it.

Adding the marketplace clones the repository (about 250 MB on disk, fonts
and gallery included); only the plugin itself (under 100 KB) is copied into
the plugin cache. A sparse clone fetches just the two directories the
plugin needs (under 1 MB):
`claude plugin marketplace add PSCatLabs/pscat --sparse .claude-plugin plugin`.

From a checkout, `claude plugin marketplace add /path/to/pscat`
works the same way.

**Already registered pscat-mcp by hand?** If you ran
`claude mcp add pscat -- pscat-mcp` before, remove it
(`claude mcp remove pscat`) so the tools are not listed twice.

## Install in Codex

From a shell (Codex CLI 0.163 or later):

```sh
codex plugin marketplace add PSCatLabs/pscat
codex plugin add pscat@pscat
```

The first command reads the Codex marketplace in
`.agents/plugins/marketplace.json`; the second copies `plugin/` (under
100 KB) into `~/.codex/plugins/cache/pscat/pscat/<version>/`. Start a new
Codex session and the skills appear as `pscat:pscat`, `pscat:psart` and
`pscat:psviz`, with the `pscat` MCP server's ten tools.
Adding the marketplace clones the whole repository (about 360 MB on
disk); a sparse clone of the two directories the plugin needs is under
2 MB:
`codex plugin marketplace add PSCatLabs/pscat --sparse .agents --sparse plugin`.
For a new release, `codex plugin marketplace upgrade pscat` refreshes the
marketplace and `codex plugin add pscat@pscat` installs the new version
(untested until the next release ships).

Depending on your approval settings, Codex asks before it calls an MCP
tool (a non-interactive `codex exec` with approvals off refuses the call).
Codex runs a plugin's hooks only after you trust them, so the
SessionStart message (the launcher's `--hook` mode) needs one review in
`/hooks`; without it a failed first download shows up only as the
server's tools being absent.

**Already registered pscat-mcp by hand?** If you ran
`codex mcp add pscat -- pscat-mcp`, remove it (`codex mcp remove pscat`).

## Platforms

Linux and Windows are tested in CI with the real Claude Code and Codex
command-line tools, and macOS by hand with the same checks; Cowork is not,
since it has no command-line form. One server entry serves every OS. Both manifests name
`scripts/pscat-mcp` with no extension:

- On macOS and Linux the agent runs that file, so it must keep its exec
  bit. A git checkout keeps it, and `scripts/plugin_check.py` checks it is
  committed as executable.
- On Windows the agent looks the extensionless name up the way Windows
  finds programs (`PATHEXT`), and so runs `scripts/pscat-mcp.cmd`. Claude
  Code does this through cross-spawn and Codex through the `which` crate.
  The `.cmd` asks `pscat-mcp.ps1` for the server's path (finding or
  downloading it), then runs `pscat-mcp.exe` itself, so PowerShell is not
  between the agent and the server.

The SessionStart hook is `cd "<plugin>/scripts"; ./pscat-mcp --hook`, which
runs the same in `sh`, Git Bash and PowerShell, the shells Claude Code uses
for hooks (Git Bash on Windows when it is installed, else PowerShell).

On Windows the launcher runs PowerShell with `-ExecutionPolicy Bypass`. A
managed machine whose Group Policy forbids that, or that puts PowerShell in
Constrained Language mode, stops the launcher. Install pscat from the
release `.zip` there, put it on `PATH`, and restart.

## Try it

- "Render a PostScript poster of a spiral galaxy and show me the result."
- "Write me a thank-you note to Sam in a hand-drawn script, on lined paper."
- "Make a bar chart of these quarterly numbers for my paper, as a PDF:
  Q1 12, Q2 18, Q3 15, Q4 22."
- "This .ps file draws a blank page. Why?"
- "Paint an autumn landscape in oil with a palette knife."
- "Type this memo on a typewriter and make it look like a declassified
  document."
- "What fonts and palettes does pscat have?"

## Privacy

Everything runs on your machine. The skills are text; the MCP server runs
your local `pscat`, which reads and writes only local files and makes no
network requests. The plugin makes one network request of its own: when no
pscat is installed, the launcher downloads the pinned release archive and
its `SHA256SUMS` from this project's GitHub releases (github.com, and
GitHub's download host), once. The other exception is
`pscat fonts install`, which you run yourself to download optional font
packs from the same releases. The plugin collects nothing and sends
nothing anywhere.

Each MCP call is limited to 60 s by default, a default and not a fixed
limit: a long render needs the server's `--timeout` (up to 86400), a raised
`--max-steps`, and the agent's own tool-call timeout raised too. The plugin's
bundled server takes no options, but it reads environment variables. Under
Claude Code, add this to `~/.claude/settings.json` (or a project's
`.claude/settings.local.json`) and restart Claude Code:

```json
{ "env": { "PSCAT_MCP_TIMEOUT": "1800", "PSCAT_MCP_MAX_STEPS": "0" } }
```

The other limits have variables too (`PSCAT_MCP_JOB_TIMEOUT`,
`PSCAT_MCP_MAX_PIXELS`, `PSCAT_MCP_MAX_MEMORY` and so on, named in the guide
below); a flag overrides its variable and an invalid value stops the server
at startup. There is deliberately no variable for `--allow-read` or
`--out-root`: these variables set only numeric limits, none adds a read root
or moves where `out_path` writes. Codex starts a
plugin's server with a short fixed environment, so under Codex (or to use
flags) register `pscat-mcp --timeout SECS` by
hand (Claude Code: `claude mcp add`, then toggle the plugin's server off in
`/mcp`; Codex: an `[mcp_servers.<name>]` table in `config.toml` with `args` and
`tool_timeout_sec`, whose default is 60, and `enabled = false` under
`[plugins."pscat@pscat".mcp_servers.pscat]` for the plugin's own). The
[agents guide](https://github.com/PSCatLabs/pscat/blob/main/docs/guides/agents.md#raising-the-time-limit)
has the settings, including Claude Code's `MCP_TOOL_TIMEOUT` and
`CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`.

The MCP tools run arbitrary PostScript as you. A program reads files only in
the project directory, the pscat data directories and any `--allow-read DIR`
the server was given; writes are confined to the project directory. See
[SECURITY.md](https://github.com/PSCatLabs/pscat/blob/main/SECURITY.md) and the
[agents guide](https://pscatlabs.github.io/pscat/agents.html) for the server's limits and
options.

## Versions

The plugin's version is pscat's version: `plugin/.claude-plugin/plugin.json`
(Claude Code) and `plugin/plugin.json` (Codex) carry the crate version, and
the release pipeline bumps them together (`scripts/plugin_check.py` fails
CI if any differ). Use the plugin with a pscat of the same version;
`pscat-mcp --version` prints yours. `/plugin update pscat@pscat` picks up
a new release in Claude Code; for Codex, see above. The release the
launcher downloads is the tag in `pscat-release`. It equals the plugin's
version, except during a pre-release trial, when it names the pre-release
(for example `v0.7.0-rc.1`).

## Layout

One directory, `plugin/`, is the plugin for both agents. It holds the
skills as real files in `skills/` (the repository's top-level `skills` is
a symlink to `plugin/skills`, so there is one copy of the text), the
shared `hooks/hooks.json`, the launcher (the entry point `scripts/pscat-mcp`,
`scripts/pscat-mcp.sh`, the Windows twin `scripts/pscat-mcp.cmd` and
`scripts/pscat-mcp.ps1`) and its pin `pscat-release`, Claude Code's
`.claude-plugin/plugin.json` (which declares the MCP server inline, as
`${CLAUDE_PLUGIN_ROOT}/scripts/pscat-mcp`), and Codex's portable
`plugin.json` and `mcp.json` (the same entry point, as `./scripts/pscat-mcp`,
the only form Codex accepts for a plugin file). Each agent ignores the other's files. There is no
top-level `bin/`: claude.ai and Cowork refuse to install a plugin that has
one.
