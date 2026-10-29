---
name: pscat
description: Render, preview, and debug PostScript with the pscat interpreter — PNG/SVG/PDF out, lint, seed sweeps, handwriting-style notes, typewritten letters and memos, photo-to-engraving, watch-it-draw window, halftone mode. Use for any .ps/.eps file work, generating diagrams or art in PostScript, producing notes, cards and lettering in a natural, hand-drawn script, or typed documents, or when the pscat MCP tools are available.
---
<!-- Contributors: tests/skill_blocks.rs runs every postscript code fence under plugin/skills on a bare install (no font packs) and fails on any error or font-pack warning. A fragment that cannot run alone is fenced as "postscript partial". -->

# pscat — PostScript as a tool

`pscat` is a PostScript (Level 2) interpreter: one binary, with its
PostScript library (`lib/*.ps`) compiled in, so `(lib/artkit.ps) run`
works from any directory. Check it is installed with `pscat --help`. With
the pscat plugin installed, `pscat` is normally on `PATH` already: the
plugin links it into `~/.local/bin` (or `~/bin`) when that directory is on
`PATH`. If `pscat` is not found, use the plugin's one stable path,
`~/Library/Caches/pscat/plugin/bin/pscat` (macOS) or
`~/.cache/pscat/plugin/bin/pscat` (Linux); `pscat-mcp` is beside it. The
plugin's session-start note names it. If that file does not exist yet, run
`sh "${CLAUDE_PLUGIN_ROOT}/scripts/pscat-mcp.sh" --locate`: it downloads
the plugin's pinned release, links it onto `PATH` and prints the `bin/`
directory. The first use downloads the release (about 14 MB, a few
seconds), so a pause then is not a hang; later runs make no request.
Without the plugin, install it:
a release from <https://github.com/PSCatLabs/pscat/releases>, or
`cargo install --git https://github.com/PSCatLabs/pscat`
(installs `pscat` and `pscat-mcp`).

**Headless on Linux:** the plugin's Linux binary has no live window, so `pscat file.ps`, `-i` and `--spool` fail there; render with `pscat --png out.png file.ps` (`cargo install pscat` builds the window).

## Render a file (the common case)

```sh
pscat --png out.png file.ps            # headless render to PNG
pscat --svg out.svg file.ps            # vector SVG (one file per page)
pscat --pdf out.pdf file.ps            # multi-page PDF
pscat --page 500x500 --png out.png f.ps  # canvas in points (default 612x792)
pscat --dpi 300 --png out.png f.ps     # print resolution (default 72)
pscat --halftone --png out.png f.ps    # screen like a mono laser printer
pscat --transparent --png out.png f.ps # no page background
generate | pscat --png out.png -       # `-` reads the program from stdin
```

Multi-page documents number their pages: `out.png` → `out-001.png`,
`out-002.png`, … SVG does the same; PDF is naturally multi-page.

Page size: `--page` always wins. Without it, a `<< /PageSize [w h] >>
setpagedevice` in the program sets the page (other page-device keys are
ignored with a warning). Without that, an EPSF file (first line
`%!PS-Adobe-3.0 EPSF-3.0`) is cropped to its `%%BoundingBox` (or
`%%HiResBoundingBox`), with the box's lower-left corner at the page's; a
plain `.ps` file gets 612x792 unless you pass `--bbox`. MCP: leave out
`width`/`height` for the same rule, `bbox: true` for `--bbox`.

Slow render? `--profile` shows where the time goes and `--bands N` splits it
over threads: [references/bands-profile.md](references/bands-profile.md).

## Show the result to the user

A render that succeeded is not an image the user has seen. A picture returned
inside a tool call (an MCP image block, a file you viewed yourself) usually shows
only in the tool trace, which hosts collapse; the user reads your final message.
When they asked to see something, the final message must contain it. Check two
things separately: the render worked (exit 0, clean `Lint:`), and the image is
in the final answer.

1. Save the PNG where the user can open it (the project or working folder), by
   `--png` or `out_path`, and keep the source `.ps` beside it when they may want
   changes. `out_path`'s reply says `wrote <absolute path>`; use that path.
2. Put the image in the final message twice over, as an embed and as a plain
   link, so that a host that does not draw the embed still gives the user the
   file, then link the source: `![a colourful spiral](ABSOLUTE_PNG_PATH)`,
   `[spiral.png](ABSOLUTE_PNG_PATH)`, `[spiral.ps](ABSOLUTE_PS_PATH)`, each with
   the real absolute path in place of the placeholder.
   - **Codex desktop:** the embed with an absolute path is what showed the image in
     the v0.7.0-rc.1 trial (#911); a tool-call image alone did not.
   - **Claude desktop (Cowork and the Code tab):** save into the user's folder and
     give the plain link; that part is documented (the Code tab opens a clicked
     path). Do not say the image is shown inline unless you can see that it is.
   - **Claude Code in a terminal:** it cannot draw images; say where the file is.
3. Never paste base64 or a `data:` URI into a reply. Do not write "rendered
   above" or "as you can see" unless the final message carries the image or path.
4. If the user says they cannot see it, do not re-render or repeat the claim:
   confirm the saved file exists, then put it in the answer as in step 2.


## Debug a program

```sh
pscat --headless file.ps               # run without rendering; errors → stderr
pscat --headless --pstack-on-error f.ps  # + gs-style operand stack post-mortem
pscat -e '3 4 add ='                   # evaluate a snippet
pscat --lint --png out.png file.ps     # self-check the render (see below)
```

Exit code 0 = clean run; nonzero = a PostScript error, reported on
stderr as `%%[ Error: undefined; OffendingCommand: bogus; Line: 3 ]%%`
(the line is given for errors in the top-level program, not inside a
`run`-loaded library). `--png` still writes the partial canvas after an
error: useful to see how far a program got.

**`--lint`** catches mistakes that don't *error*: a blank page, an
unbalanced `gsave`/`grestore`, stuff left on the operand/dict stack.
Findings print to stderr as `pscat: lint: [check] message` (or
`pscat: lint: clean`); the exit code is unchanged (`--lint-strict` makes
findings fail). A run that errored prints `pscat: lint: [run-error]`.

Seed and parameter sweeps (`--sweep-seed 1:12 --contact-sheet grid.png`) and
labelled side-by-side sheets of several files or sizes (`pscat sheet`):
[references/sheets.md](references/sheets.md).

## Fonts

```sh
pscat --fonts                          # every findfont-reachable face
```

**All of the standard 35 (the LaserWriter 35) are built in**, so a bare
install has them: Helvetica, Times and Courier; Palatino, Bookman,
NewCenturySchlbk, AvantGarde, ZapfChancery and Helvetica-Narrow; Symbol
and ZapfDingbats. Everything else is a font pack, installed with
`pscat fonts install` (all packs) or `--pack <name>`:

| Pack | Faces |
|---|---|
| `extras` | the display families (`/EBGaramond-Regular`, `/Bangers`, `/GreatVibes-Regular`, …), Courier Prime, Special Elite, JetBrains Mono |
| `cjk-thai` | the Korean, Japanese and Thai faces |

`pscat --fonts` lists what this install can reach. Plugin installs have no
packs, so **design with the standard 35** unless the user has run
`pscat fonts install`. A name that is not available never fails the
render. Two cases:

- **A name from a pack that is not installed** (`/Bangers`) falls back to
  a built-in face, keeping weight and slant: the monospaced extras
  (Courier Prime, Special Elite, JetBrains Mono) become Courier; the other
  `extras` faces and the Korean/Japanese/Thai faces become Helvetica,
  which cannot draw those scripts. pscat prints one line on stderr per
  name, `pscat: warning: font /Bangers is in the extras font pack, which
  is not installed; using Helvetica. Run ...` with the
  `pscat fonts install --pack extras` command, and the run exits 0. Treat
  that line as a defect in a sketch meant to work on a bare install.
- **A name nothing knows** (`/NoSuchFont`) becomes Helvetica with no
  warning, so check `--fonts` before relying on a face.

With the pack installed the real face is used and nothing changes. The Type
3 program faces load from the built-in library anywhere:
`(lib/fonts/neon.ps) run` (/Neon, /Marquee, /Constellation, /Lapidary,
/Circuitry, /Stitchwork, /Confetti).

`pscat --capabilities` prints the machine-readable inventory of what this build
has (fonts, palettes, templates, procedures, native operators) as JSON; prefer
it over guessing a name from prose, this file included.

## What else pscat makes (read the file when the task calls for it)

Each line says when to open which reference file in `references/`.

- **Notes, cards, lettering in a hand-drawn script** (`pscat handwrite "text" -o
  note.png`; lowercase only) and **a formula on a chalkboard** (`pscat chalk
  'x = \frac{1}{2}' -o board.png`, single backslashes):
  [references/handwriting-chalk.md](references/handwriting-chalk.md). The
  handwriting is for generic hands only: it is not for imitating a named or real
  person's handwriting or passing writing off as someone else's. Decline that part
  of a request and offer a generic hand (the default /HandScript, another `--seed`).
- **Typed letters, memos, pages, an aged "released document"** (`pscat typewrite
  letter.txt -o letter.png`):
  [references/typewriter.md](references/typewriter.md).
- **Seed or parameter sweeps, contact sheets, labelled review sheets** comparing
  files or sizes (`pscat sheet`, `review_sheet`):
  [references/sheets.md](references/sheets.md).
- **A slow render** (`--profile`, `--bands`):
  [references/bands-profile.md](references/bands-profile.md).
- **pscat-only operators and small tools**: `setalpha` / `setblendmode`,
  native noise (`gnoise2`, `fbm2`, ...), `writereals` / `readreals` for big
  arrays, `pscat photo-etch` (photo to engraving), the live window (`pscat
  file.ps`, `-i`, `--spool`), the `--capabilities` inventory:
  [references/extensions.md](references/extensions.md).
- **MCP details**: render jobs, time limits, `bands`, the `limit_exceeded`
  error, registering the server:
  [references/mcp.md](references/mcp.md).
- **Art** (palettes, turtle, L-systems, brushes, paint and dry media,
  chalkboards, TeX on a board): the `psart` skill.
- **Charts and plots for papers** (`lib/dataviz.ps`, `lib/graph.ps`): the
  `psviz` skill.

## MCP

`pscat-mcp` (installed beside `pscat`) serves ten tools over stdio:
`render_postscript` (source → PNG images inline, or SVG text, or a PDF
written to `out_path`; adds a `Lint:` block when the page looks wrong),
`eval_postscript`, `handwrite`, `chalkboard_math` (LaTeX → a chalkboard PNG),
`review_sheet`, `describe_art_capabilities`, plus render jobs for anything slow
(`start_render`, `render_status`, `render_result`, `cancel_render`; each call
has a 60 s limit by default). `out_path` must be inside the server's output
root (the server's instructions name it; `--out-root DIR`, which the plugin
takes from `out-root = DIR` in `~/.config/pscat/plugin.conf`), end in the output
format's extension (`.png`, `.svg`, `.pdf`) and not sit in a dot-directory such
as `.git/`, and large inline images are refused: lower `dpi` or pass `out_path`.
A workspace outside the root: save there, then `cp` the file; never decode an
inline image. Jobs, limits, `bands` and registering the server:
[references/mcp.md](references/mcp.md).

## Pitfalls

- **No `showpage`, no page** in Ghostscript. pscat's `--png` is
  forgiving (it writes trailing art), but add `showpage` to any file
  that should also work in gs.
- `--page WxH` is in points and sets the canvas; `--dpi` scales device
  resolution without changing coordinates. For a 2× PNG of a 612x792
  page use `--dpi 144`, not `--page 1224x1584`. Canvases are capped at
  8000×8000 device pixels.
- pscat is not a sandbox: a program can read any file you can read
  (`(path) (r) file`; the MCP tools confine reads to the working directory,
  the pscat data directories and `--allow-read DIR`), and on the command line nothing stops an infinite
  loop unless you ask: `--max-steps N`, `--max-time SECS` and `--max-memory
  MIB` stop a runaway with a `limitexceeded` error. Run untrusted PostScript
  as you would an untrusted script.
- Writing files is off by default. On the command line, `--allow-write DIR`
  (repeatable) lets a program write inside DIR with `(DIR/f) (w) file`,
  `(a) file`, `renamefile` and `deletefile`; anything outside is
  `invalidfileaccess`. `(%stdout) (w) file` needs no flag. The MCP tools
  never write from PostScript: use their `out_path` instead.
- The interpreter targets Level 2 PostScript. If a construct misbehaves,
  compare against Ghostscript, the behavioural oracle.
- `cvs` (and `=`, `==`) on a real prints the shortest digits that read back
  exactly, up to 17 significant digits, so the text can be 24 characters
  (`-3.3333333333333334e-301`). Ghostscript prints six. Size the string for
  that (`30 string`), or round first. `0.38612390214204787 12 string cvs`
  is a `rangecheck` here; rounding it to `0.386` first and then printing
  with `30 string` gives `(0.386)`.
- Files the docs call `examples/...`, `gallery/...` or `docs/...` are in
  the [repository](https://github.com/PSCatLabs/pscat),
  not in an install.
- Big numeric arrays: use `writereals` / `readreals`, not printed text
  ([references/extensions.md](references/extensions.md)).
