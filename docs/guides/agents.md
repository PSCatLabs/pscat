# pscat for AI agents

pscat is easy to drive from a coding agent — Claude Code, Codex,
OpenClaw, Hermes, or anything else. Two integration surfaces:

**Docs-reading agents**: [`.claude/skills/pscat/SKILL.md`](../../.claude/skills/pscat/SKILL.md) is the
one-page tool reference (Claude Code picks it up automatically as a
skill; it's plain markdown, so any agent can read it). Two more skills sit
beside it: [`psart`](../../.claude/skills/psart/SKILL.md) teaches the
render-look-refine loop for generative art, and
[`psviz`](../../.claude/skills/psviz/SKILL.md) teaches paper-ready charts and
plots. [`AGENTS.md`](../../AGENTS.md) points to the pscat skill as well, which
is where Codex looks first.

**MCP-wired agents**: `pscat-mcp` (built alongside `pscat`) is an
MCP server over stdio exposing four tools:

- `render_postscript` — source in; PNG image(s) back inline (or SVG
  text, or a PDF written to `out_path`), with page size, `dpi`, and
  `halftone` options. Errors still return the partial render. Also
  runs the self-check/lint pass (`--lint` on the CLI) and
  appends a `Lint:` block when it finds something — a blank page, an
  unbalanced `gsave`, stuff left on the stack — so a whole class of
  silent failure shows up without eyeballing the PNG.
- `handwrite` — text in, handwritten-note PNG back (the
  `scripts/handwrite.sh` options: size, paper, ink, jitter, seed).
- `describe_art_capabilities` — the machine-readable catalog of what
  this build's creative toolkit actually has installed: fonts, Type 3
  program faces, mood palettes, page templates, and artkit/style-pack
  procedures (`--capabilities` on the CLI). See
  [`docs/dev/CAPABILITIES.md`](../dev/CAPABILITIES.md).
- `eval_postscript` — run headlessly, get back what the program
  printed, or the standard error name plus an operand-stack
  post-mortem.

Register it:

```sh
cargo build --release
claude mcp add pscat -- $PWD/target/release/pscat-mcp     # Claude Code
codex mcp add pscat -- $PWD/target/release/pscat-mcp      # Codex
```

For OpenClaw, Hermes, or any other MCP client, the generic stdio
config is:

```json
{ "mcpServers": { "pscat": { "command": "/path/to/target/release/pscat-mcp" } } }
```

**Safety.** The tools run arbitrary PostScript as you, which can read any file you
can read, and `out_path` writes (or overwrites) any path you can write. The server
has no timeouts or confinement yet. Read [`SECURITY.md`](../../SECURITY.md) before
you hand it to an agent you do not trust.

The server shells out to the `pscat` CLI (found next to it), so the
tools always match the CLI's behavior. `handwrite` also needs
`scripts/handwrite.sh`, looked for in `$PSCAT_HOME` (or `$PSCAT_ROOT`),
the per-user data directory, next to the binaries, and three levels above
`pscat-mcp`, which is the checkout when it is `target/release/pscat-mcp`
(see [Where pscat finds its files](getting-started.md#where-pscat-finds-its-files)).
A release bundle or the checkout recipe above needs no extra setup; set
`PSCAT_HOME` only if you've moved the binaries away from their
`lib`/`fonts`/`scripts` siblings. (The script lookup moves into `pscat`
itself with #600.)
