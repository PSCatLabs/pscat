<!-- Contributors: the pscat-cloud ChatGPT plugin (scripts/build_chatgpt_plugin.py --remote, #1256) ships this skill with the edits below applied to plugin/skills/pscat/SKILL.md; no other build sees them. An edit whose anchor no longer matches the shared text fails the build: update it here. The hosted server has seven tools and no command line, no local files and no jobs. -->
<!-- cloud: sub ", seed sweeps, handwriting-style notes, typewritten letters and memos, photo-to-engraving, watch-it-draw window, halftone mode." -->
, handwriting-style notes, chalkboard formulas, review sheets, halftone mode.
<!-- cloud: replace "`pscat` is a PostScript (Level 2) interpreter" -->
`pscat` is a PostScript (Level 2) interpreter with its PostScript library
(`lib/*.ps`) built in, so `(lib/artkit.ps) run` works in any program. In this
plugin it runs on a hosted server and you reach it through the MCP tools named
below: nothing is installed, and there is no command line and no local file.
<!-- cloud: drop "**Headless on Linux:**" -->
<!-- cloud: replace "## No MCP, or a cloud sandbox" -->
## The hosted tools

Everything goes through the hosted pscat server's seven MCP tools. ChatGPT
lists them under this plugin's name, pscat-cloud:

- `render_postscript`: a PostScript program (`source`) in, its pages out as PNG
  (the default), SVG or PDF (`format`), with each page's size, the dpi and
  whatever the program printed. Also `width` and `height` (points), `dpi` and
  `halftone` (PNG only).
- `validate_postscript`: runs a program without returning an image and reports
  the PostScript error (name, offending command, line), lint findings, the page
  count and what it printed.
- `describe_art_capabilities`: the fonts, palettes, page templates, procedures,
  native operators and dials this build has, with parameters and examples.
- `get_renderer_info`: the renderer version, output formats, the limits in
  force and the installed font packs.
- `handwrite`: text in, a handwriting-style note out (PNG).
- `chalkboard_math`: LaTeX in, a formula on a chalkboard out (PNG, SVG or PDF).
- `review_sheet`: several programs, each at one or more sizes, in one labelled
  PNG for side-by-side comparison.

These seven are all there is: call no other pscat tool name. Programs run in a
sandbox that reads only the built-in `lib/`, writes nothing and has no network,
and each render has a time limit (`get_renderer_info` reports it), so keep
programs small or lower the `dpi`.

**If these tools are not available** in the conversation, or a call fails to
connect, tell the user that the pscat-cloud connection is not working and quote
the error. Do not try to install, download or run pscat yourself, and do not
draw the picture with another renderer unless the user asks for that.

The command-line options that other pscat guides mention map onto the tools:
`--page WxH` is `width` and `height`; `--dpi N` is `dpi`; `--svg` and `--pdf`
are `format`; `--halftone` is `halftone`; `--lint` and `--headless` are
`validate_postscript`; `--capabilities` and `--fonts` are
`describe_art_capabilities`; `pscat sheet` is `review_sheet`; `pscat handwrite`
and `pscat chalk` are `handwrite` and `chalkboard_math`. Typewritten pages, sweeps
(`--sweep-seed`), photo-to-engraving, the live window and `--bands` have no
tool: write the PostScript for them yourself, as the reference files describe.
<!-- cloud: drop "## In the browser or Node" -->
<!-- cloud: sub "## Render a file (the common case)" -->
## Render with `render_postscript` (the common case)
<!-- cloud: replace "pscat --png out.png file.ps            # headless render to PNG" -->
Call `render_postscript` with the program as `source`, and add what the piece
needs:

- `width` and `height`: the page in points (the default is 612x792).
- `dpi`: the resolution (the default is 72, one pixel per point; a 4 inch card
  at 300 dpi is 1200 pixels).
- `format`: `png` (one image per page, the default), `svg` (one document per
  page) or `pdf` (one document holding every page).
- `halftone`: `true` screens the raster like a mono laser printer (PNG only).

The reply gives each page's size in points and pixels, and the program's output.
<!-- cloud: replace "Multi-page documents number their pages" -->
A program that shows several pages gives one PNG or SVG per page, up to 10
pages, or one PDF holding all of them.
<!-- cloud: sub "With `--pdf` (or `pdf: true` in the browser build), the Distiller" -->
With `format: "pdf"`, the Distiller
<!-- cloud: sub ", a file opened with `file` (read roots apply; not in the browser build), or inline" -->
 or inline
<!-- cloud: sub " On PNG/SVG/window output pdfmark only clears to the mark." -->
 On PNG and SVG output pdfmark only clears to the mark.
<!-- cloud: sub " (Needs the pscat release that includes PR #1073.)" -->
 (Needs a recent enough renderer; `get_renderer_info` gives the version.)
<!-- cloud: replace "Page size: `--page` always wins." -->
Page size: `width` and `height` always win. Without them, a `<< /PageSize [w h]
>> setpagedevice` in the program sets the page (other page-device keys are
ignored with a warning), and otherwise the page is 612x792.
<!-- cloud: replace "Slow render? `--profile` shows where the time goes" -->
A render that passes the time limit fails: keep the program small, lower the
`dpi`, or do less work per pixel. There are no `--bands` or `--profile`
options here: [references/bands-profile.md](references/bands-profile.md) says
what to do instead.
<!-- cloud: replace "## Show the result to the user" -->
## Show the result to the user

A render that succeeded is not an image the user has seen. `render_postscript`
returns the page to you, the model, and a picture inside a tool call may show
only in the tool trace; the user reads your final message. Check two things
separately: the render worked (no PostScript error, no lint finding you did not
intend), and the user has what they asked for.

1. The hosted tools write no file and take no output path, so there is nothing
   to save or link on the server. Say what you drew, and when the user may want
   to change or reuse the piece, give the PostScript source in the reply: it is
   the editable original, and the same program with the same seed gives the
   same picture.
2. Do not write "rendered above" or "as you can see" unless you can see that
   the image reached the user. If they say they cannot see it, do not re-render
   or repeat the claim: say that the tool's image did not display, give the
   source, and offer an SVG render (`format: "svg"`), whose text can be put in
   a reply or a file.
3. Never paste base64 or a `data:` URI into a reply.
<!-- cloud: replace "pscat --headless file.ps               # run without rendering" -->
`validate_postscript` runs the program and returns the PostScript error (name,
offending command and line), the lint findings, the page count and what the
program printed, without an image. `render_postscript` returns the same error
instead of an image, and returns what the program printed. To see a value,
print it with `=` or `==` in the program and read that output: there is no
separate evaluate tool.
<!-- cloud: replace "Exit code 0 = clean run; nonzero" -->
An error comes back as its name, the offending command and the line, for
example `undefined` with `bogus` on line 3 (an error while a `run` file is
being read gives that file's line and name). To find it in a long program,
run `validate_postscript` on shorter prefixes.
<!-- cloud: sub "**`--lint`** catches mistakes" -->
**The lint check** (in `validate_postscript`'s reply) catches mistakes
<!-- cloud: sub "Findings print to stderr as `pscat: lint: [check] message` (or
`pscat: lint: clean`); the exit code is unchanged (`--lint-strict` makes
findings fail). A run that errored prints `pscat: lint: [run-error]`." -->
Each finding names its check in brackets, as `[shadowed-operator]` does; a
program that errored is reported as `[run-error]`.
<!-- cloud: replace "Seed and parameter sweeps (`--sweep-seed 1:12" -->
Labelled side-by-side sheets of several programs or sizes (`review_sheet`), and
how to compare seeds with it: [references/sheets.md](references/sheets.md).
<!-- cloud: replace "pscat --fonts                          # every findfont-reachable face" -->
`describe_art_capabilities` with `kind` set to `font` lists every face the
server can reach, and `get_renderer_info` names the installed font packs.
<!-- cloud: sub "Everything else is a font pack, installed with
`pscat fonts install` (all packs) or `--pack <name>`:" -->
Everything else is a font pack, present only if the server has installed it:
<!-- cloud: sub "`pscat --fonts` lists what this install can reach. Plugin installs have no
packs, so **design with the standard 35** unless the user has run
`pscat fonts install`." -->
`describe_art_capabilities` lists what this server can reach. **Design with the
standard 35** unless it shows that a pack is installed.
<!-- cloud: replace-item "- **A name from a pack that is not installed**" -->
- **A name from a pack that is not installed** (`/Bangers`) falls back to
  a built-in face, keeping weight and slant: the monospaced extras
  (Courier Prime, Special Elite, JetBrains Mono) become Courier; the other
  `extras` faces and the Korean/Japanese/Thai faces become Helvetica,
  which cannot draw those scripts. The reply may carry a warning naming the
  font and its pack, and the call still succeeds. Treat that warning as a
  defect in a sketch meant to work without the pack.
<!-- cloud: sub "so check `--fonts` before relying on a face." -->
so check `describe_art_capabilities` before relying on a face.
<!-- cloud: drop "**Your own font files**" -->
<!-- cloud: replace "`pscat --capabilities` prints the machine-readable inventory of what this build" -->
`describe_art_capabilities` returns the machine-readable inventory of what this
build has (fonts, palettes, templates, procedures, native operators); prefer it
over guessing a name from prose, this file included.
<!-- cloud: replace "## Where `(lib/...) run` finds a file" -->
## Where `(lib/...) run` finds a file

A `lib/` name loads the library compiled into the server. There is no per-user
`lib/`, no environment variable that redirects it, and no other file a program
can read, so a library of your own has to be pasted into the program's source.
<!-- cloud: replace "## What else pscat makes (read the file when the task calls for it)" -->
## What else pscat makes (read the file when the task calls for it)

Each line says when to open which reference file in `references/`.

- **Notes, cards, lettering in a hand-drawn script** (`handwrite`; lowercase
  only) and **a formula on a chalkboard** (`chalkboard_math`):
  [references/handwriting-chalk.md](references/handwriting-chalk.md). The
  handwriting is for generic hands only: it is not for imitating a named or real
  person's handwriting or passing writing off as someone else's. Decline that part
  of a request and offer a generic hand (the default, another `seed`).
- **Typed letters, memos, pages, an aged "released document"** (PostScript
  libraries, no tool):
  [references/typewriter.md](references/typewriter.md).
- **Labelled review sheets** comparing programs, sizes or seeds (`review_sheet`):
  [references/sheets.md](references/sheets.md).
- **A slow render**:
  [references/bands-profile.md](references/bands-profile.md).
- **pscat-only operators**: `setalpha` / `setblendmode` and native noise
  (`gnoise2`, `fbm2`, ...): [references/extensions.md](references/extensions.md).
- **The seven tools in full**, their limits and what they return:
  [references/mcp.md](references/mcp.md).
- **Art** (palettes, turtle, L-systems, brushes, paint and dry media,
  chalkboards, TeX on a board): the `psart` skill.
- **Charts and plots for papers** (`lib/dataviz.ps`, `lib/graph.ps`): the
  `psviz` skill.
<!-- cloud: drop "## MCP" -->
<!-- cloud: sub "- `--page WxH` is in points and sets the canvas; `--dpi` scales device
  resolution without changing coordinates. For a 2× PNG of a 612x792
  page use `--dpi 144`, not `--page 1224x1584`. Canvases are capped at
  8000×8000 device pixels." -->
- `width` and `height` are in points and set the canvas; `dpi` scales device
  resolution without changing coordinates. For a 2× PNG of a 612x792
  page use `dpi` 144, not `width` 1224 and `height` 1584. Canvases are capped at
  8000×8000 device pixels.
<!-- cloud: replace-item "- pscat is not a sandbox" -->
- The server is a sandbox, but a program can still loop until a limit stops it:
  the render's time limit, and its step and memory limits, end a runaway with a
  limit error. A program can read no file except the built-in `lib/`, and has no
  clock or network.
<!-- cloud: drop-item "- Writing files is off by default." -->
<!-- cloud: drop-item "- Big numeric arrays: use" -->
<!-- cloud: sub "pscat's `--png` is
  forgiving" -->
pscat's render is
  forgiving
