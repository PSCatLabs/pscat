# pscat for AI agents

pscat is easy to drive from a coding agent — Claude Code, Codex,
OpenClaw, Hermes, or anything else. There are two integration surfaces,
and both work against an **installed** pscat (no checkout needed):

- **Skills**: plain Markdown in [`plugin/skills/`](../../plugin/skills/) (also
  reachable as `skills/` in a checkout), one directory
  per skill, each with a `SKILL.md` (Claude Code's skill format; any agent
  can read it). [`pscat`](../../plugin/skills/pscat/SKILL.md) is the tool
  reference, [`psart`](../../plugin/skills/psart/SKILL.md) the render-look-refine
  loop for art (a short page plus `references/` for paint, dry media,
  board math and style packs, read only when needed), and
  [`psviz`](../../plugin/skills/psviz/SKILL.md) paper-ready charts. They call the
  `pscat` binary and its subcommands (`pscat handwrite`, `pscat typewrite`,
  `pscat photo-etch`) and load the library with `(lib/...) run`, which reads
  the copy compiled into `pscat`. [`AGENTS.md`](../../AGENTS.md) points
  Codex at the pscat skill.
- **MCP**: `pscat-mcp`, installed beside `pscat`, an MCP server over stdio
  with six tools (below).

In Claude Code the easiest route is the **plugin**, which installs the three
skills and registers `pscat-mcp` in one step (install pscat first; the plugin
uses the binaries on your PATH):

```text
/plugin install pscat --marketplace PSCatLabs/pscat
```

or `/plugin marketplace add PSCatLabs/pscat` and then
`/plugin install pscat@pscat`.

In Codex, the same plugin comes from the repository's Codex marketplace
(`.agents/plugins/marketplace.json`):

```sh
codex plugin marketplace add PSCatLabs/pscat
codex plugin add pscat@pscat
```

[`plugin/README.md`](../../plugin/README.md) has the details for both:
requirements, example prompts, privacy, and what happens when pscat is
missing.

`plugin/skills/` is the single source of the skill text: both plugins
install from `plugin/`, and the top-level `skills` is a symlink to it. The
skills are real files there, not a symlink, because Codex skips symlinks
when it copies a plugin into its cache. The repository's own development skills (`work-issue`, `sdlcify`,
`issue-summary`) live in `.claude/skills/` and are not published; that
directory holds symlinks to the three user-facing skills so Claude Code
sessions in a checkout still load them.

## What pscat does, and where an agent finds it

| Job | MCP tool | CLI | Skill section |
|---|---|---|---|
| Render a PostScript file or snippet, see the page | `render_postscript` | `pscat --png/--svg/--pdf` | pscat: Render a file |
| Debug a program: errors, stack, lint | `eval_postscript`; `render_postscript`'s `Lint:` block | `pscat --headless --pstack-on-error`, `--lint` | pscat: Debug a program |
| Compare seeds or a parameter | (loop `render_postscript`) | `--sweep-seed`, `--sweep`, `--contact-sheet` | pscat: Sweep |
| Review sheet: several files or sizes, labelled | `review_sheet` | `pscat sheet` | pscat: Build a labelled review sheet |
| Handwriting-style note | `handwrite` | `pscat handwrite` | pscat: Handwriting-style notes |
| Typed letter or memo | `render_postscript` with `tw-type` | `pscat typewrite` | pscat: Typewritten pages |
| "Released" / declassified document look | `render_postscript` with `rel-page` | — | pscat: Typewritten pages |
| Chart or plot for a paper | `render_postscript` | `pscat --png` | psviz |
| Formula or equation on a chalkboard (LaTeX in) | `chalkboard_math` | `pscat chalk` | pscat: LaTeX on a chalkboard |
| Chalk formula inside a larger picture | `render_postscript` with `mkpath` + `dcchalk` | — | psart: references/board-math.md, references/dry-media.md |
| Generative art (turtle, L-systems, scatter, tiling, fractals) | `render_postscript` | `pscat --png` | psart: The toolkit |
| Painting (oil, knife, watercolor, wet-on-wet, landscapes) | `render_postscript` | — | psart: references/paint.md |
| Chalk, crayon, charcoal drawing | `render_postscript` | — | psart: references/dry-media.md |
| Print looks (etching, stipple, hatch, halftone, photocopy) | `render_postscript` (`halftone` flag for screening) | `--halftone` | psart: Media and kits |
| Photo → line engraving | — | `pscat photo-etch` | pscat: Photo → engraving |
| Lettering, headlines, apparel mockups | `render_postscript` | — | psart: Media and kits |
| Style packs (steampunk, psychedelic, sci-fi, toon), Type 3 faces | `render_postscript` | — | psart: references/styles-and-type.md |
| Korean, Japanese, Thai text | `render_postscript` (needs the font catalog) | — | pscat: Fonts |
| What fonts, palettes, templates, procedures exist | `describe_art_capabilities` | `pscat --capabilities`, `--fonts` | pscat: Capabilities |
| PDF retype | planned (#559, #560) | — | — |

Task-level MCP tools are added only where agents clearly do better with
one than with `render_postscript` plus the skill's snippet (#623).
`chalkboard_math` is the first: LaTeX in, one call, instead of a PostScript
program with every backslash doubled. `review_sheet` is the second: several
programs in, one labelled sheet out, instead of a render call per size and a
PIL composite. A one-call typewriter or chart is not added yet.

## The MCP server

- `render_postscript` — source in; PNG image(s) back inline (or SVG text,
  or a PDF written to `out_path`), with page size, `dpi` and `halftone`
  options. Errors still return the partial render with the error text.
  Runs the lint pass and appends a `Lint:` block when it finds something
  (a blank page, an unbalanced `gsave`, stuff left on the stack).
- `handwrite` — text in, handwriting-style note PNG back (the `pscat handwrite`
  options: size, width, paper, ink, jitter, seed, dpi; the note is refused
  past `--max-pixels`, see the limits below). It is a set of
  hand-drawn styles for notes, cards and art, not a way to imitate a specific
  person's handwriting or pass writing off as someone else's.
- `chalkboard_math` — a LaTeX formula in (single backslashes, `\\` rows,
  `&` alignment), a chalkboard PNG back: the `pscat chalk` board (#555), with
  its config keys as optional arguments (`size`, `irregular`, `slope`,
  `jitter`, `align`, `color`, `board` or `false` for a transparent page,
  `ghosting`, `seed`, `width`/`height`, `dpi`, `format`, and `config` for the
  full `--json` schema). Same input and seed, the same bytes. A command
  outside mathkit's subset is a one-line `latex_error` naming it (`code`
  `unsupported_command`, `token` `\oplus`). The input is LaTeX, never
  PostScript. By default `latex` is capped at 4096 bytes and the board at 40
  million pixels (`--max-latex-bytes`, `--max-pixels`; see the limits below); svg and pdf go to `out_path` only.
- `review_sheet` — several programs in, one labelled PNG out: the
  `pscat sheet` sheet (#693), byte for byte. `entries` is a list of rows,
  each `{source, label, width, height, dpi}` (`source` is the program, never
  a file name; `label` defaults to `program N`); `width`, `height` and `dpi`
  at the top level are the defaults for rows that give none. `dpi` is a list
  of cells, left to right, each a number (9..1200) or `{"dpi": 9, "zoom": 4}`
  to draw that render again enlarged 4x with no smoothing, so a small icon
  can be read pixel by pixel. Every cell is drawn at its true pixel size,
  centred in its column, with the row label above and a `48x48 px` caption
  below (`labels: false`, `sizes: false` drop them). A program that errors is
  drawn as far as it got, with its error text beside the image. Each cell is
  its own `pscat` child under the interpreter limits and the one call
  `--timeout`. At most 32 cells, the cells and the sheet at most `--max-pixels`
  (40 million by default), 8000 a side (`output_too_large`); see the limits below. The #683
  logo sheet is one call:
  `{"entries": [{"source": "<a.ps>", "label": "A: one-line cat"}, ...],
  "width": 384, "height": 384, "dpi": [96, 9, {"dpi": 9, "zoom": 4}]}`.
- `describe_art_capabilities` — the machine-readable catalog of what this
  build has: fonts, Type 3 faces, palettes, page templates, procedures
  (`--capabilities` on the CLI; see
  [`docs/dev/CAPABILITIES.md`](../dev/CAPABILITIES.md)).
- `eval_postscript` — run headlessly, get back what the program printed,
  or the error (name, offending command, line) plus an operand-stack
  post-mortem.

### Tool annotations

Every tool declares the MCP `annotations` hints, so a client can decide when
to ask before calling it (and a directory review can check them). All five
fields are set on every tool:

| Tool | `readOnlyHint` | `destructiveHint` | `idempotentHint` | `openWorldHint` |
|---|---|---|---|---|
| `describe_art_capabilities` | true | false | true | false |
| `eval_postscript` | true | false | true | false |
| `render_postscript` | false | false | true | false |
| `handwrite` | false | false | true | false |
| `chalkboard_math` | false | false | true | false |
| `review_sheet` | false | false | true | false |

The hints are static per tool, so the four tools that can write a file take the conservative
value: they write a file only when `out_path` is given, but without a way to
say "read-only unless this argument is set" they are marked `readOnlyHint:
false`. They are not destructive: `out_path` is confined to the output root and
replaces only an existing render file of the same extension (see below).
`openWorldHint` is false everywhere because no tool reaches the network.
`read-only` here means the tool writes nothing; `eval_postscript`'s program (and
a render's) can still *read* any local file you can, which each description says.
`tests/mcp.rs` keeps the same table and fails for a tool missing from it.

The Claude Code plugin registers it for you. To register it by hand instead
(after installing pscat, so `pscat-mcp` is on your PATH; not both, or the
tools are listed twice):

```sh
claude mcp add pscat -- pscat-mcp          # Claude Code
codex mcp add pscat -- pscat-mcp           # Codex
```

`pscat mcp` is the same server under the `pscat` binary, for clients that
launch a crate by its name; the [MCP Registry](https://registry.modelcontextprotocol.io/)
lists pscat as `io.github.PSCatLabs/pscat` (from 0.6.3) and runs it that way.

For OpenClaw, Hermes, or any other MCP client, the generic stdio config:

```json
{ "mcpServers": { "pscat": { "command": "pscat-mcp", "args": ["--out-root", "/path/to/output"] } } }
```

### Limits and options

Set by whoever registers the server; no tool argument can change them.

| Option | Default | What it bounds |
|---|---|---|
| `--timeout SECS` | 60 | Wall-clock time per tool call, 1 to 86400 (the 60 is only the default; see "Raising the time limit" below). The interpreter stops a program a margin before it (a quarter of it, at most 5 s: 55 s by default; `limit_exceeded`, `limit: "time"`, and a render keeps the partial page); the child is killed at the full time, for a program stuck inside one long operator, and the call fails with `timeout`. |
| `--max-steps N` | 25000000000 | Interpreter steps per program (objects executed, plus loop iterations). Past it the interpreter stops and the call fails with `limit_exceeded`, `limit: "steps"`. `0` = off. |
| `--max-memory MIB` | 1024 | Heap the program may grow by, in MiB (`limit_exceeded`, `limit: "memory"`). `0` = off. |
| `--max-input-bytes N` | 1048576 | `source` / `text` size (`input_too_large`). |
| `--max-pixels N` | 40000000 | The largest page, board or sheet in device pixels (width and height in points times dpi/72) for `render_postscript`, `handwrite`, `chalkboard_math` and `review_sheet`, checked before a canvas is allocated (`output_too_large`, with `limit_pixels`, `pixels`, `width_px`, `height_px`). 1 to 64000000; 8000 pixels a side always applies. An 8K screen or an A2 poster at 300 dpi fits the default. |
| `--max-latex-bytes N` | 4096 | `chalkboard_math`'s `latex` size (`input_too_large`, with `limit`); about 40 rows. |
| `--max-image-bytes N` | 3750000 | Inline image bytes per call, about 5 MB once base64-encoded; also at most 20 pages inline (`output_too_large`; a file asked for with `out_path` is still written). |
| `--out-root DIR` | the working directory | Where `out_path` may write. Off when the server starts in `/`. |

`chalkboard_math` checks its caps before anything runs: `latex` at most
`--max-latex-bytes` (default 4096: about 40 rows, four times a long
derivation; the time limit stops anything slower), and the board at most
`--max-pixels` (default 40,000,000) and 8000 a side. `render_postscript` and
`handwrite` take the same pixel cap: a `render_postscript` page is refused
before a child starts, and a `handwrite` note, whose height is sized from its
text, before it is drawn. All are in `SECURITY.md`.

`review_sheet` has caps too, checked before any program runs
(`output_too_large`): 32 cells (`limit_cells`), the cells at most
`--max-pixels` in all and the sheet at most `--max-pixels` and 8000 a side
(`limit_pixels`), a label at most 200 characters, and the sources together at
most `--max-input-bytes`. The cells share the one `--timeout`. Also in
`SECURITY.md`.

The three interpreter limits (steps, time, memory) are on by default for
`render_postscript`, `eval_postscript` and `chalkboard_math`; a program cannot catch them with
`stopped`. They are read between interpreter steps, so one operator that runs
for a long time (an enormous fill, a read of a device file) is outside them;
`SECURITY.md` lists what each limit can and cannot stop. `handwrite` runs
trusted library code on escaped text and is bounded by `--timeout` and
`--max-pixels`.

Program output is capped at 4 MiB per stream (`output_too_large`). A failed
call is an MCP tool error (`isError: true`) whose text says what happened,
with the same facts as `structuredContent.error` (`kind`, `message`, and for
PostScript errors `ps_error`, `offending_command`, `line`; a program stopped by
an interpreter limit has kind `limit_exceeded` and a `limit` field).

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

### Raising the time limit

The 60 s is a default, not a fixed limit, and three things have to allow a
long render. A call that takes minutes (a ray-marched landscape) works over
MCP once all three do; a job you want to poll or cancel, or one past the
server's 24-hour cap, is the async-jobs work still open in #741.

1. **The server's `--timeout SECS`** (1 to 86400), the time limit above.
   Register the server with it, for example
   `claude mcp add pscat -- pscat-mcp --timeout 1800 --max-steps 0`.
   The interpreter's own stop moves with it (a quarter of it, at most 5 s,
   before `--timeout`).
2. **The other interpreter limits.** `--max-steps` defaults to 25 billion,
   which at about 60 million steps a second is roughly seven minutes: a long
   `--timeout` alone ends in `limit_exceeded` with `limit: "steps"` long
   before the time is up. Raise it, or `--max-steps 0` for none. A big canvas
   may also want `--max-memory` (MiB, default 1024) and `--max-pixels`.
3. **The MCP client's own tool-call timeout**, which cuts a call off whatever
   the server allows:
   - *Claude Code*: `MCP_TOOL_TIMEOUT`, in milliseconds, defaults to about 28
     hours, so it rarely binds. A per-server `timeout` field in milliseconds
     (in `.mcp.json`, or via `claude mcp add-json`) overrides it for one
     server. A stdio server that sends no response and no progress
     notification for 30 minutes is aborted by an idle timeout, and
     `pscat-mcp` sends no progress notifications, so for a call past 30 minutes
     also set `CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT` (milliseconds; `0` turns it
     off) or a per-server `timeout` of at least that long. (`MCP_TIMEOUT` is
     the server *startup* timeout, a different setting.)
   - *Codex*: `tool_timeout_sec` in the server's `[mcp_servers.pscat]` table
     of `config.toml`. Its default is **60**, so under Codex the client cuts a
     call at 60 s even when `--timeout` is higher. For example:
     ```toml
     [mcp_servers.pscat]
     command = "pscat-mcp"
     args = ["--timeout", "1800", "--max-steps", "0"]
     tool_timeout_sec = 1800
     ```
   - *Any other client*: look for its tool-call or request timeout.

The plugins' bundled server is registered without options, so it keeps the
60 s default. To raise it, register the server by hand as above and, so the
tools are not listed twice, toggle the plugin's server off in `/mcp` (Claude
Code) or remove it (Codex).
The settings above are from the Claude Code
([MCP](https://code.claude.com/docs/en/mcp),
[environment variables](https://code.claude.com/docs/en/env-vars)) and Codex
([MCP](https://developers.openai.com/codex/mcp)) docs as of October 2026.

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
