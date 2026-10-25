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
- **The `pscat-mcp` server**, which gives the agent six tools:
  `render_postscript` (pages come back as images the agent can look at),
  `eval_postscript`, `handwrite`, `chalkboard_math` (LaTeX in, a chalkboard
  PNG back), `review_sheet` (several programs at several sizes, labelled, on
  one PNG) and `describe_art_capabilities`.

pscat's handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's.

## Requirements: install pscat first

The plugin does not ship binaries. It runs the `pscat` and `pscat-mcp`
you have installed, found on your `PATH`. Either:

```sh
cargo install pscat
```

or download a release archive from
<https://github.com/PSCatLabs/pscat/releases> and put
`pscat` and `pscat-mcp` on your `PATH`. Check with `pscat --help`.

If they are missing, the plugin still installs and the skills still load,
but the MCP server cannot start. A SessionStart hook (`hooks/hooks.json`, which
runs `hooks/check-pscat.sh`; both Claude Code and Codex read it) then starts each session with a
message saying `pscat`/`pscat-mcp` was not found and how to install it.
In Claude Code, `/mcp` lists `plugin:pscat:pscat` as failed. Install pscat
and restart your agent.

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
`pscat:psviz`, with the `pscat` MCP server's six tools.
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
missing-pscat message (the `hooks/check-pscat.sh` script) needs one review
in `/hooks`; without it a missing `pscat-mcp` shows up only as the
server's tools being absent.

**Already registered pscat-mcp by hand?** If you ran
`codex mcp add pscat -- pscat-mcp`, remove it (`codex mcp remove pscat`).

## Platforms

macOS and Linux are tested; Windows is not. The plugin directory has no
symbolic links, so a Windows checkout no longer loses the skills, but the
missing-pscat hook is a POSIX `sh` script (`hooks/check-pscat.sh`) and
needs a POSIX shell to run (#633).

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
network requests. The one exception is `pscat fonts install`, which you
run yourself to download optional font packs from this project's GitHub
releases. The plugin collects nothing and sends nothing anywhere.

Each MCP call is limited to 60 s by default, a default and not a fixed
limit: a long render needs the server's `--timeout` (up to 86400), a raised
`--max-steps`, and the agent's own tool-call timeout raised too. The plugin's
bundled server takes no options, so register `pscat-mcp --timeout SECS` by
hand and toggle the plugin's server off in `/mcp` (Claude Code), or set
`tool_timeout_sec` for the server (Codex, default 60). The
[agents guide](https://pscatlabs.github.io/pscat/agents.html#raising-the-time-limit)
has the settings, including Claude Code's `MCP_TOOL_TIMEOUT` and
`CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`.

The MCP tools run arbitrary PostScript as you, so a program can read any
file you can read; writes are confined to the project directory. See
[SECURITY.md](https://github.com/PSCatLabs/pscat/blob/main/SECURITY.md) and the
[agents guide](https://pscatlabs.github.io/pscat/agents.html) for the server's limits and
options.

## Versions

The plugin's version is pscat's version: `plugin/.claude-plugin/plugin.json`
(Claude Code) and `plugin/plugin.json` (Codex) carry the crate version, and
the release pipeline bumps them together (`scripts/plugin_check.py` fails
CI if any differ). Use the plugin with a pscat of the same version;
`pscat-mcp --version` prints yours. `/plugin update pscat@pscat` picks up
a new release in Claude Code; for Codex, see above.

## Layout

One directory, `plugin/`, is the plugin for both agents. It holds the
skills as real files in `skills/` (the repository's top-level `skills` is
a symlink to `plugin/skills`, so there is one copy of the text), the
shared `hooks/hooks.json` and `hooks/check-pscat.sh`, Claude Code's `.claude-plugin/plugin.json`
(which declares the MCP server inline), and Codex's portable `plugin.json`
and `mcp.json`. Each agent ignores the other's files.
