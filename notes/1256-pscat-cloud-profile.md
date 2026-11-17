# 1256: a cloud-only profile for the pscat-cloud ChatGPT ZIP

Builds on #1254/#1255 (the remote build is the `pscat-cloud` plugin). The owner's
#1089 test showed ChatGPT labelling the uploaded plugin **Desktop only**
although `mcp.json` declares only a `streamable-http` server. We have not shown
that the bundled scripts cause that label; it may come from OpenAI's
registration whatever the ZIP holds, and only the owner can see it in ChatGPT.
The cleanup is justified anyway: the bundled instructions told an agent to
download and run a local pscat, which the hosted setup contradicts.

## What `--remote` now does

- Leaves out `scripts/` (the local launchers), `pscat-release` (a binary version
  pin) and `skills/pscat-chatgpt-setup/`. `REMOTE_EXCLUDE` in the builder is the
  profile. Kept: `plugin.json`, `mcp.json`, the icon and the four skills with
  their references.
- Applies the overlay in `plugin-chatgpt/cloud/` to the shared skill files.

## Why an overlay and not markers in the shared skills

`plugin/skills/` is installed verbatim by Claude Code and Codex (and read by
`tests/skill_blocks.rs`, `plugin_check.py`, the install checks, the site pages
that link it, and the `.claude/skills/` symlinks),
so a `<!-- cloud-only -->` marker there would reach the desktop text, and no
consumer strips it. The overlay keeps `plugin/` untouched: the local and
`--skills-only` ZIPs were compared with `cmp` against the previous builder's
output on the base commit and are identical, and `git diff` of `plugin/` is
empty. The price is that an edit names the shared text it replaces, so a
reworded sentence breaks the build until the overlay is updated (the tests
report the file and anchor). The guidance that is not local-only (the
PostScript language notes, fonts, QR, pitfalls, the art toolkit) is not
duplicated: an edit touches only the passage that needs it.

Edit forms: `sub` (an exact text, once), `replace`/`drop` (a `#` heading's
section, or the one blank-line block holding the anchor), `replace-item`/
`drop-item` (a list item with its wrapped lines) and `replace-file` (whole
file; used for the references that are entirely about the CLI or the local
server: `mcp.md`, `sheets.md`, `bands-profile.md`, `handwriting-chalk.md`).
A block with a code fence, or an anchor matching 0 or 2 places, fails.

## The hosted tool facts the text uses

From the server's `tools/list` on 2026-11-17: `render_postscript` takes
`source`, `width`, `height`, `dpi`, `format` (png, svg, pdf) and `halftone`;
there is no `out_path`, `bands`, `bbox` or `gpu_preview`; no `eval_postscript`,
job tools, or file output; programs read only the built-in `lib/`;
`validate_postscript` runs the program and returns lint and stdout. If a tool
is added or an argument changes, edit `plugin-chatgpt/cloud/skills/pscat/`.
The server also serves the skills as `pscat://guide/` resources; this ZIP does
not use them.

## What changed in the text

- The `pscat` skill: intro, a "hosted tools" section (the seven tools, what to
  do when they are not connected: report it, never install or run pscat, never
  substitute another renderer), render and debug sections in terms of tool
  arguments, show-the-result without `out_path`, fonts, the lib search order,
  pitfalls; the install, "No MCP", "In the browser or Node" and "MCP" sections
  are gone.
- `references/`: `mcp.md`, `sheets.md`, `bands-profile.md`,
  `handwriting-chalk.md` are rewritten for the tools; `typewriter.md` and
  `extensions.md` lose their CLI sections and keep the PostScript ones.
- `psart` and `psviz`: the loop and the command-line examples point at
  `render_postscript`, `review_sheet`, `validate_postscript` and
  `describe_art_capabilities`; the "With an image model" section (it reads a JPEG
  file) is dropped. `functional-postscript`: one line.

## Not claimed

Whether ChatGPT still says **Desktop only** for this ZIP, and whether a render
goes straight to the hosted tools, are the owner's checks in ChatGPT (#1089).
