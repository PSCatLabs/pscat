# pscat for AI agents

pscat is easy to drive from a coding agent — Claude Code, Codex,
OpenClaw, Hermes, or anything else. There are two integration surfaces,
and both work against an **installed** pscat (no checkout needed):

- **Skills**: plain Markdown in [`skills/`](../../skills/), one directory
  per skill, each with a `SKILL.md` (Claude Code's skill format; any agent
  can read it). [`pscat`](../../skills/pscat/SKILL.md) is the tool
  reference, [`psart`](../../skills/psart/SKILL.md) the render-look-refine
  loop for art (a short page plus `references/` for paint, dry media,
  board math and style packs, read only when needed), and
  [`psviz`](../../skills/psviz/SKILL.md) paper-ready charts. They call the
  `pscat` binary and its subcommands (`pscat handwrite`, `pscat typewrite`,
  `pscat photo-etch`) and load the library with `(lib/...) run`, which reads
  the copy compiled into `pscat`. [`AGENTS.md`](../../AGENTS.md) points
  Codex at the pscat skill.
- **MCP**: `pscat-mcp`, installed beside `pscat`, an MCP server over stdio
  with four tools (below).

In Claude Code the easiest route is the **plugin**, which installs the three
skills and registers `pscat-mcp` in one step (install pscat first; the plugin
uses the binaries on your PATH):

```text
/plugin install pscat --marketplace jeromebanks/postscript_interpreter
```

or `/plugin marketplace add jeromebanks/postscript_interpreter` and then
`/plugin install pscat@pscat`. [`plugin/README.md`](../../plugin/README.md)
has the details: requirements, example prompts, privacy, and what happens
when pscat is missing.

`skills/` is the single source for the plugin (`plugin/skills` is a symlink
to it), so there is one copy of the skill text. The repository's own development skills (`work-issue`, `sdlcify`,
`issue-summary`) live in `.claude/skills/` and are not published; that
directory holds symlinks to the three user-facing skills so Claude Code
sessions in a checkout still load them.

## What pscat does, and where an agent finds it

| Job | MCP tool | CLI | Skill section |
|---|---|---|---|
| Render a PostScript file or snippet, see the page | `render_postscript` | `pscat --png/--svg/--pdf` | pscat: Render a file |
| Debug a program: errors, stack, lint | `eval_postscript`; `render_postscript`'s `Lint:` block | `pscat --headless --pstack-on-error`, `--lint` | pscat: Debug a program |
| Compare seeds or a parameter | (loop `render_postscript`) | `--sweep-seed`, `--sweep`, `--contact-sheet` | pscat: Sweep |
| Handwritten note | `handwrite` | `pscat handwrite` | pscat: Handwritten notes |
| Typed letter or memo | `render_postscript` with `tw-type` | `pscat typewrite` | pscat: Typewritten pages |
| "Released" / declassified document look | `render_postscript` with `rel-page` | — | pscat: Typewritten pages |
| Chart or plot for a paper | `render_postscript` | `pscat --png` | psviz |
| Formula on a chalkboard | `render_postscript` with `mkpath` + `dcchalk` | — | psart: references/board-math.md, references/dry-media.md |
| Generative art (turtle, L-systems, scatter, tiling, fractals) | `render_postscript` | `pscat --png` | psart: The toolkit |
| Painting (oil, knife, watercolor, wet-on-wet, landscapes) | `render_postscript` | — | psart: references/paint.md |
| Chalk, crayon, charcoal drawing | `render_postscript` | — | psart: references/dry-media.md |
| Print looks (etching, stipple, hatch, halftone, photocopy) | `render_postscript` (`halftone` flag for screening) | `--halftone` | psart: Media and kits |
| Photo → line engraving | — | `pscat photo-etch` | pscat: Photo → engraving |
| Lettering, headlines, apparel mockups | `render_postscript` | — | psart: Media and kits |
| Style packs (steampunk, psychedelic, sci-fi, toon), Type 3 faces | `render_postscript` | — | psart: references/styles-and-type.md |
| Korean, Japanese, Thai text | `render_postscript` (needs the font catalog) | — | pscat: Fonts |
| What fonts, palettes, templates, procedures exist | `describe_art_capabilities` | `pscat --capabilities`, `--fonts` | pscat: Capabilities |
| LaTeX → chalkboard, PDF retype | planned (#555, #559, #560) | — | — |

Task-level MCP tools (a one-call typewriter or chart, say) are deliberately
not added yet: `render_postscript` with the skill's snippet does each job,
and a dedicated tool is only worth it where an eval shows agents doing
clearly better with it (#623).

## The MCP server

- `render_postscript` — source in; PNG image(s) back inline (or SVG text,
  or a PDF written to `out_path`), with page size, `dpi` and `halftone`
  options. Errors still return the partial render with the error text.
  Runs the lint pass and appends a `Lint:` block when it finds something
  (a blank page, an unbalanced `gsave`, stuff left on the stack).
- `handwrite` — text in, handwritten-note PNG back (the `pscat handwrite`
  options: size, width, paper, ink, jitter, seed, dpi).
- `describe_art_capabilities` — the machine-readable catalog of what this
  build has: fonts, Type 3 faces, palettes, page templates, procedures
  (`--capabilities` on the CLI; see
  [`docs/dev/CAPABILITIES.md`](../dev/CAPABILITIES.md)).
- `eval_postscript` — run headlessly, get back what the program printed,
  or the error (name, offending command, line) plus an operand-stack
  post-mortem.

The Claude Code plugin registers it for you. To register it by hand instead
(after installing pscat, so `pscat-mcp` is on your PATH; not both, or the
tools are listed twice):

```sh
claude mcp add pscat -- pscat-mcp          # Claude Code
codex mcp add pscat -- pscat-mcp           # Codex
```

For OpenClaw, Hermes, or any other MCP client, the generic stdio config:

```json
{ "mcpServers": { "pscat": { "command": "pscat-mcp", "args": ["--out-root", "/path/to/output"] } } }
```

### Limits and options

Fixed by whoever registers the server; no tool argument can change them.

| Option | Default | What it bounds |
|---|---|---|
| `--timeout SECS` | 60 | Wall-clock time per tool call; the child is killed past it and the call fails with `timeout`. |
| `--max-input-bytes N` | 1048576 | `source` / `text` size (`input_too_large`). |
| `--max-image-bytes N` | 3750000 | Inline image bytes per call, about 5 MB once base64-encoded; also at most 20 pages inline (`output_too_large`; a file asked for with `out_path` is still written). |
| `--out-root DIR` | the working directory | Where `out_path` may write. Off when the server starts in `/`. |

Program output is capped at 4 MiB per stream (`output_too_large`). A failed
call is an MCP tool error (`isError: true`) whose text says what happened,
with the same facts as `structuredContent.error` (`kind`, `message`, and for
PostScript errors `ps_error`, `offending_command`, `line`).

**`out_path`** is resolved against the output root (relative paths start
there; an absolute path must land inside it). `..` is refused, the directory
must already exist, must not resolve outside the root through a symlink and
must not be (or sit below) a dot-directory such as `.git/`, and an existing
target that is not a regular file is refused. The name must end in the
extension of the output format (`.png`, `.svg`, `.pdf`; `.png` for
`handwrite`); a wrong or missing extension is refused, naming the extension
to use. The file is written to a temp file beside the target and renamed over
it, so an existing regular file with that extension is replaced and nothing
else can be overwritten. Claude Code and Codex start servers in
the project directory, so by default an agent can write renders into the
project and nowhere else.

### Protocol

The legacy `initialize` handshake, for the revisions the server has been
tested with: **2025-11-25** (Claude Code 2.1, the MCP Inspector) and
**2025-06-18** (Codex). A request for another well-formed revision is
answered with 2025-11-25, as the spec's negotiation requires (the client
decides whether to continue); a missing or malformed `protocolVersion` is
refused with error -32602 and `data.supported`. The stateless 2026-07-28
revision is not implemented yet (#621): its `server/discover` probe gets
"method not found", which dual-era clients such as Claude Code take as the
signal to fall back to `initialize`. CI drives the server with the
official MCP Inspector CLI (`.github/workflows/mcp.yml`,
`scripts/mcp_inspector_check.sh`).

**Safety.** The tools run arbitrary PostScript as you, which can read any
file you can read, and return what it printed. Writes are confined as
above. Read [`SECURITY.md`](../../SECURITY.md) before you hand the server
to an agent you do not trust.

The server shells out to the `pscat` CLI found next to it, so the tools
always match the CLI's behavior, and it needs nothing but the two binaries:
no checkout, no `scripts/`, no shell (it works on Windows). `lib/` is
compiled into `pscat`; see
[Where pscat finds its files](getting-started.md#where-pscat-finds-its-files)
for the font catalog.
