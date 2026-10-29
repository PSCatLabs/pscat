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
  with ten tools (below).

In Claude Code the easiest route is the **plugin**, which installs the three
skills and registers `pscat-mcp` in one step. No separate install is needed
on macOS or Linux: the plugin's launcher (`plugin/scripts/pscat-mcp.sh`)
uses a pscat you already have (PATH, `~/.cargo/bin`, Homebrew), and otherwise
downloads the pinned GitHub release once, checks it against the release's
`SHA256SUMS` and caches it (`~/Library/Caches/pscat/plugin` or
`~/.cache/pscat/plugin`; the stable path of the command-line tool is `bin/pscat`
there). It also links `pscat` and `pscat-mcp` into `~/.local/bin` or `~/bin`
when that directory is already on `PATH`, so a plain `pscat` works. The same plugin installs in the Claude desktop app
(Customize > Plugins > Add marketplace, `PSCatLabs/pscat`), where Cowork runs
the server on your computer. In Claude Code:

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
| A render longer than one call allows (minutes, up to an hour by default) | `start_render`, then `render_status` / `render_result` | `pscat --png` | pscat: MCP |
| Painting (oil, knife, watercolor, wet-on-wet, landscapes) | `render_postscript` | — | psart: references/paint.md |
| Chalk, crayon, charcoal drawing | `render_postscript` | — | psart: references/dry-media.md |
| Print looks (etching, stipple, hatch, halftone, photocopy) | `render_postscript` (`halftone` flag for screening) | `--halftone` | psart: Media and kits |
| Photo → line engraving | — | `pscat photo-etch` | pscat: Photo → engraving |
| Lettering, headlines, apparel mockups | `render_postscript` | — | psart: Media and kits |
| Style packs (steampunk, psychedelic, sci-fi, toon), Type 3 faces | `render_postscript` | — | psart: references/styles-and-type.md |
| Korean, Japanese, Thai text | `render_postscript` (needs the font catalog) | — | pscat: Fonts |
| What fonts, palettes, templates, procedures, native operators exist | `describe_art_capabilities` | `pscat --capabilities`, `--fonts` | pscat: Capabilities |
| PDF retype | planned (#559, #560) | — | — |

Task-level MCP tools are added only where agents clearly do better with
one than with `render_postscript` plus the skill's snippet (#623).
`chalkboard_math` is the first: LaTeX in, one call, instead of a PostScript
program with every backslash doubled. `review_sheet` is the second: several
programs in, one labelled sheet out, instead of a render call per size and a
PIL composite. A one-call typewriter or chart is not added yet.

## The MCP server

- `render_postscript` — source in; PNG image(s) back inline (or SVG text,
  or a PDF written to `out_path`), with page size, `dpi`, `halftone` and
  `bands` options (`bands`: see "Banded renders" below). Errors still return the partial render with the error text.
  Runs the lint pass and appends a `Lint:` block when it finds something
  (a blank page, an unbalanced `gsave`, stuff left on the stack).
  `render_postscript`, `handwrite`, `chalkboard_math`, `review_sheet` and
  `eval_postscript` also
  return what a *successful* run wrote to stderr (a font falling back to a
  built-in face, say) as a `Warnings:` text item: each distinct line once, at
  most 8 lines of 300 characters, without the child's own receipt.
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
- `eval_postscript` — run headlessly, get back what the program printed
  (plus a `Warnings:` item for anything it wrote to stderr), or the error (name, offending command, line) plus an operand-stack
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
The PostScript itself can never write a file: the server never passes the CLI's
`--allow-write`, so `(path) (w) file`, `writereals`, `renamefile` and `deletefile` fail with
`invalidfileaccess` (#740). `%stdout` writes come back as printed output.
Reading is confined to the server's **read roots** (#763): its working directory
(unless that is `/`), the pscat data directories (`$PSCAT_HOME` and the per-user one,
where font packs install) when they exist, and each `--allow-read DIR`.
`(path) (r) file`, `run`, `readreals` and image or data sources opened by name
read only files that resolve inside one of them; anything else, a `..` anywhere in
the name, a symlink leading out, or `/dev/zero`, is `invalidfileaccess`, and so is
a missing file named by an absolute path outside (the error never says whether
it exists). `filenameforall`
lists only inside the roots: an absolute template elsewhere, or one with `..`,
yields no names (no error; the PLRM gives it no `invalidfileaccess`, and
Ghostscript's `-dSAFER` skips files it may not open the same way). The bundled
`lib/` is not a disk read and always works. The server passes the roots to each
render as the CLI's `--confine-reads` and `--read-root DIR`; the CLI itself reads
openly unless given those. `read-only` here means the tool writes nothing.
No tool takes an input file path: `review_sheet` and the rest take PostScript
source, not files, so the roots are the only way in.
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

Every numeric option below can also be set with an environment variable, so a
plugin user can change a limit without registering the server by hand (see
"Raising the time limit"): `PSCAT_MCP_` plus the flag name in capitals with
underscores, for example `PSCAT_MCP_TIMEOUT`, `PSCAT_MCP_JOB_TIMEOUT`,
`PSCAT_MCP_MAX_JOBS`, `PSCAT_MCP_JOB_TTL`, `PSCAT_MCP_MAX_BANDS`, `PSCAT_MCP_MAX_INPUT_BYTES`,
`PSCAT_MCP_MAX_IMAGE_BYTES`, `PSCAT_MCP_MAX_PIXELS`,
`PSCAT_MCP_MAX_LATEX_BYTES`, `PSCAT_MCP_MAX_STEPS` and `PSCAT_MCP_MAX_MEMORY`.
A variable takes the same values as its flag, and a flag overrides its
variable. A value outside the flag's range (or not a whole number) stops the
server at startup, naming the variable; an empty value counts as unset.
**`--out-root` and `--allow-read` have no `PSCAT_MCP_*` variable, on
purpose**: these variables set only numeric limits, and none adds a read root
or moves the output root, so a settings file cannot widen file access through
them (the same rule as for writes from PostScript, #740). Because they can
raise the caps (or turn the step and memory limits off), set them only in
settings you trust.

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
| `--allow-read DIR` | none (repeatable) | Another directory programs may read from, besides the working directory (not `/`) and the pscat data directories. A directory that doesn't exist stops the server at startup. |

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

Program output is capped at 4 MiB per stream (`output_too_large`). `==`, `pstack`
and `stack` truncate their own output well below that (1024 elements per array,
4096 bytes per string, 16 KiB per operand, 32 KiB for a whole stack, ending in
`... truncated`), and an error's `offending_command` is cut at 512 bytes. A failed
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

The 60 s is a default, not a fixed limit. For a render that takes minutes
(a ray-marched landscape), there are two ways:

- **A render job** (below): `start_render` returns at once, so no call is
  long and no client timeout applies. Only the job's own limit
  (`--job-timeout`, an hour by default) and the interpreter limits of step 2
  below matter. This is the way under Codex, whose client cuts any call at
  60 s by default.
- **One long call**, once all three of the following allow it:

1. **The server's `--timeout SECS`** (1 to 86400), the time limit above, or
   its environment variable `PSCAT_MCP_TIMEOUT`.
   - *With the plugin installed (Claude Code), no hand registration:* put the
     variables in your Claude Code settings (`~/.claude/settings.json` for
     every project, or `.claude/settings.local.json` for one) and restart Claude
     Code, because the server reads them when it starts:
     ```json
     { "env": { "PSCAT_MCP_TIMEOUT": "1800", "PSCAT_MCP_MAX_STEPS": "0" } }
     ```
     Claude Code copies the `env` block into its own environment, and a stdio
     MCP server inherits it (exporting the variables in the shell that starts
     `claude` does the same). The one exception is if
     `CLAUDE_CODE_MCP_ALLOWLIST_ENV=1` is set: Claude Code then starts stdio
     servers with only a safe baseline environment plus the server's own
     configured `env`, so the variables never arrive.
   - *Registering the server by hand* takes the flags, for example
     `claude mcp add pscat -- pscat-mcp --timeout 1800 --max-steps 0`, and a
     flag overrides a variable.
   - *Codex:* the plugin route does not work. Codex starts a stdio server with
     only a short list of variables (`HOME`, `PATH`, `LANG`, `TERM`, `TMPDIR`
     and the like) plus the `env`/`env_vars` named in that server's own
     `config.toml` entry, and user config cannot add `env` to a plugin's
     server (as of October 2026, from the Codex docs and its `rmcp-client`
     source). Use a hand-registered `[mcp_servers.pscat]` with `args`, below.
   The interpreter's own stop moves with it (a quarter of it, at most 5 s,
   before `--timeout`).
2. **The other interpreter limits.** `--max-steps` defaults to 25 billion,
   which at about 60 million steps a second is roughly seven minutes: a long
   `--timeout` alone ends in `limit_exceeded` with `limit: "steps"` long
   before the time is up. Raise it (`PSCAT_MCP_MAX_STEPS`), or `--max-steps 0`
   for none. A big canvas may also want `--max-memory` (MiB, default 1024) and
   `--max-pixels` (`PSCAT_MCP_MAX_MEMORY`, `PSCAT_MCP_MAX_PIXELS`).
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
60 s default per call (and the hour per job) unless the `PSCAT_MCP_*`
variables above reach it: under Claude Code, a settings `env` block does that
with the plugin installed as it is. To use flags instead, or under Codex,
register the server by hand as above and, so the
tools are not listed twice, toggle the plugin's server off in `/mcp` (Claude
Code), or in Codex's `config.toml` set `enabled = false` under
`[plugins."pscat@pscat".mcp_servers.pscat]` (documented by Codex for plugin
servers; user config cannot change a plugin server's command, so its
arguments stay the plugin's).
The settings above are from the Claude Code
([MCP](https://code.claude.com/docs/en/mcp),
[environment variables](https://code.claude.com/docs/en/env-vars)) and Codex
([MCP](https://developers.openai.com/codex/mcp)) docs as of October 2026.

### Render jobs

For a render longer than one call may take (#741). A job is
`render_postscript` run in the background:

1. `start_render` takes `render_postscript`'s arguments and returns
   `{"job_id": "job-1", "state": "queued", ...}` at once. Bad arguments are
   refused here, before a job exists.
2. `render_status` (`job_id`) answers at once, even while renders run:
   `state` (`queued`, `running`, `done`, `failed`, `cancelled`),
   `elapsed_seconds`, `pages_done` (pages ended by `showpage` so far; a
   single page shows 0 while it is drawn, and a program that never calls
   `showpage` shows 0 throughout), `cancel_requested` after a cancel, and a failed
   job's `error`. Poll it at whatever pace suits the client.
3. `render_result` (`job_id`) returns exactly what `render_postscript` would
   have: the images, a `PostScript error:` block with a partial page,
   `Lint:`, `Warnings:`, the file written. Before the job is finished it is a
   `job_not_finished` error; for a cancelled job, `cancelled`. It can be
   fetched again until the job expires.
4. `cancel_render` (`job_id`) stops a queued job before it starts, or kills a
   running one's child; `render_status` reads `cancelled` once the child is
   gone.

| Option | Default | What it bounds |
|---|---|---|
| `--job-timeout SECS` | 3600 | Wall-clock time per job, 1 to 86400, in place of `--timeout` (the interpreter stops it a margin before, as for a call). `--max-steps`, `--max-memory` and `--max-pixels` apply as to a call: raise `--max-steps` (or `0`) for a long render. |
| `--max-jobs N` | 2 | Jobs running at once, 1 to 16; more wait, `queued`, and start in the order they were asked for. |
| `--max-bands N` | 4 | Most threads one render may ask for with `bands` (on `render_postscript` and `start_render`), 1 to 64; 1 turns banded renders off. A larger request is refused, not lowered. At most `--max-jobs` jobs plus 4 calls run at once, so the worst case is (`--max-jobs` + 4) x `--max-bands` render threads. |
| `--job-ttl SECS` | 3600 | How long a finished job (its result) is kept, 1 to 86400. At most 16 jobs are kept in all: a new job drops the oldest finished one, and is refused with `busy` when none has finished. |

Results are held in the server's memory, and no job outlives the server:
when stdin closes (how a client stops a stdio server) every job is
cancelled and its child reaped before the server exits, and each child also
stops itself if the server is killed outright (Unix; `--exit-with-parent`).
Job ids are `job-1`, `job-2`, ... per server: guessable, which is fine for a
local stdio server with one client. The hosted server (#689) keeps a low cap
and does not offer jobs yet.

**Calls run side by side.** Every tool call but the job tools runs on its
own thread, so `ping` and other calls are answered during a long render, and
`notifications/cancelled` stops that call's child (no reply is sent for a
cancelled call, as the spec asks). At most 4 calls run at once; past that a
call is refused with `busy`.

**MCP Tasks.** The MCP spec has a native form of this: *tasks*,
experimental in 2025-11-25 (`tools/call` with a `task` parameter, then
`tasks/get`, `tasks/result`, `tasks/cancel`), moved in 2026-07-28 into the
`io.modelcontextprotocol/tasks` extension (`tools/call` answered with
`resultType: "task"` and a `taskId`, then `tasks/get`, whose `completed`
answer carries the result, and `tasks/cancel`). As checked on 2026-10-25,
Claude Code (2.1.304 and later, public beta) supports the extension on
2026-07-28 connections only, and does not use the 2025-11-25 form; Codex
(0.164.0) supports neither. pscat-mcp speaks 2025-11-25 and 2025-06-18
(#621 tracks 2026-07-28), so it uses plain tools, which every client can
call. Once it speaks 2026-07-28, the jobs map onto the extension directly:
`start_render` is a `tools/call` answered with a task (`job_id` = `taskId`,
`queued`/`running` = `working`, `done` = `completed`, `failed`, `cancelled`),
`render_status` and `render_result` are `tasks/get`, `cancel_render` is
`tasks/cancel`, and `--job-ttl` is `ttlMs`. Sources: the
[2025-11-25 tasks utility](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks),
the [2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog),
[Claude Code's MCP docs](https://code.claude.com/docs/en/mcp), and a search
of the Codex source at `rust-v0.164.0`.

### Banded renders

`render_postscript` and `start_render` take `bands` (#843): the PNG is
rendered as `pscat --bands N` does (see the getting-started guide), on N
threads. The page is cut into about 2 bands per thread (none under 8 device
rows), and each band runs the whole program with `/BandIndex`, `/BandCount`
and `/BandBBox` defined in `userdict`. Only a program that reads `/BandBBox`
and skips work outside its band gets faster; any other pays for the whole
program once per band, about 2 x `bands` times the work. The image is byte
for byte the one `bands: 1` gives, unless the program reads the clock.

- **Limits.** The server's `--max-time` (derived from `--timeout` or
  `--job-timeout`) and `--max-steps` apply to **each band**; `--max-memory`
  to all bands together, though not to each running band's own canvas, so
  a banded render can hold up to `bands` canvases (each up to `--max-pixels`
  x 4 bytes) beyond the budget. The first band to hit a limit stops the queue, so a
  runaway program still stops after about one limit's worth of time. The
  server's wall-clock kill (`--timeout` for a call, `--job-timeout` for a
  job) covers the **whole** render: with about 2 bands per thread queued,
  bands that each take most of the per-band time limit can add up past it,
  and the kill returns `timeout` with no partial page. Size such a render to
  finish well inside the call's or job's limit.
- **What it gives up.** PNG only: `bands` above 1 with `format` `svg` or
  `pdf` is refused (`invalid_argument`), as is a value over `--max-bands` or
  over the page's device rows. A banded job's `pages_done` stays 0 until it
  ends: `pscat --bands` refuses `--progress`, so the server leaves it off.
  The `Lint:` block is there as for a plain render (#847): the server passes
  `--lint`, which checks band 0 (page count, gsave, stack) and the stitched
  page (blank page), and the block opens with a line saying so when there
  is a finding. Errors come back as for a plain render, from the first band
  that failed.
- **No `band_rows`.** `pscat --band-rows R` is not exposed. The default split
  measured best (`notes/844-bands-work-queue.md`); a small R multiplies the
  CPU (18x at one row per band), and a large band count multiplies the
  per-band step and time budgets. An agent has no measurement to tune it by.

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

**Safety.** The tools run arbitrary PostScript as you and return what it
printed. Reads are confined to the read roots and writes to `out_path`, as
above. Read [`SECURITY.md`](../../SECURITY.md) before you hand the server
to an agent you do not trust.

The server shells out to the `pscat` CLI found next to it, so the tools
always match the CLI's behavior, and it needs nothing but the two binaries:
no checkout, no `scripts/`, no shell (it works on Windows). `lib/` is
compiled into `pscat`; see
[Where pscat finds its files](getting-started.md#where-pscat-finds-its-files)
for the font catalog.
