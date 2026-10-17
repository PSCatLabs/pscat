# pscat plugin for Claude Code

Render, preview and debug PostScript, make generative art, and draw
paper-ready charts from Claude Code, with the
[pscat](https://github.com/jeromebanks/postscript_interpreter) PostScript
interpreter. The plugin bundles:

- **Three skills**: `pscat` (the tool reference: render to PNG/SVG/PDF,
  debug, lint, seed sweeps, handwritten notes, typed letters and memos,
  photo-to-engraving), `psart` (the render-look-refine loop for generative
  art, painting, dry media, chalkboard maths and style packs) and `psviz`
  (function plots and bar/line/area/scatter/pie charts for papers).
- **The `pscat-mcp` server**, which gives Claude four tools:
  `render_postscript` (pages come back as images Claude can look at),
  `eval_postscript`, `handwrite` and `describe_art_capabilities`.

## Requirements: install pscat first

The plugin does not ship binaries. It runs the `pscat` and `pscat-mcp`
you have installed, found on your `PATH`. Either:

```sh
cargo install --git https://github.com/jeromebanks/postscript_interpreter
```

or download a release archive from
<https://github.com/jeromebanks/postscript_interpreter/releases> and put
`pscat` and `pscat-mcp` on your `PATH`. Check with `pscat --help`.

If they are missing, the plugin still installs and the skills still load,
but the MCP server cannot start: each session starts with a message
saying `pscat`/`pscat-mcp` was not found and how to install it, and
`/mcp` lists `plugin:pscat:pscat` as failed. Install pscat and restart
Claude Code.

## Install the plugin

In Claude Code:

```text
/plugin marketplace add jeromebanks/postscript_interpreter
/plugin install pscat@pscat
```

On Claude Code 2.1.275 or later, one line does both:

```text
/plugin install pscat --marketplace jeromebanks/postscript_interpreter
```

From a shell, the same thing is
`claude plugin marketplace add jeromebanks/postscript_interpreter` and
then `claude plugin install pscat@pscat`. Restart Claude Code (or run
`/reload-plugins`) to load it.

Adding the marketplace clones the whole repository (about 140 MB with its
fonts and gallery). Only the plugin itself (under 100 KB) is copied into
the plugin cache. To skip most of the clone:
`claude plugin marketplace add jeromebanks/postscript_interpreter --sparse .claude-plugin plugin skills`.

From a checkout, `claude plugin marketplace add /path/to/postscript_interpreter`
works the same way.

**Already registered pscat-mcp by hand?** If you ran
`claude mcp add pscat -- pscat-mcp` before, remove it
(`claude mcp remove pscat`) so the tools are not listed twice.

**Windows:** the plugin's `skills` directory is a symbolic link into the
repository's `skills/`. Git for Windows checks symlinks out as plain files
unless symlink support is on (`git config --global core.symlinks true`
with Developer Mode enabled), and without it the skills do not load. The
MCP server works either way. Only macOS and Linux have been tested.

## Try it

- "Render a PostScript poster of a spiral galaxy and show me the result."
- "Write me a handwritten thank-you note to Sam, on lined paper."
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

The MCP tools run arbitrary PostScript as you, so a program can read any
file you can read; writes are confined to the project directory. See
[SECURITY.md](../SECURITY.md) and the
[agents guide](../docs/guides/agents.md) for the server's limits and
options.

## Versions

The plugin's version is pscat's version: `plugin/.claude-plugin/plugin.json`
carries the crate version and the release pipeline bumps both together
(`scripts/plugin_check.py` fails CI if they differ). Use the plugin with a
pscat of the same version; `pscat-mcp --version` prints yours.
`/plugin update pscat@pscat` picks up a new release.
