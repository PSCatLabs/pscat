<!-- Contributors: edits the pscat-cloud ChatGPT plugin applies to plugin/skills/psart/SKILL.md (scripts/build_chatgpt_plugin.py --remote, #1256); an anchor that no longer matches the shared text fails the build. The hosted server has seven tools, no command line and no local files. -->
<!-- cloud: sub "You have a full PostScript interpreter (`pscat` on your PATH), its
library (`lib/*.ps`, compiled into the binary, so `(lib/artkit.ps) run`
works from any directory), seven Type 3 display faces, and a font
catalog. Art here is *written*, rendered, looked at, and refined. The
`pscat` skill has the command-line reference; this one is the workflow
and the toolkit." -->
You have a full PostScript interpreter on a hosted server (the `pscat` skill
names its tools), its library (`lib/*.ps`, built in, so `(lib/artkit.ps) run`
works in any program), seven Type 3 display faces, and a font
catalog. Art here is *written*, rendered, looked at, and refined. The
`pscat` skill has the tool reference; this one is the workflow
and the toolkit.
<!-- cloud: replace "1. Write the piece as a `.ps` file" -->
1. Write the piece as a PostScript program (start from the sketch at the end).
2. Call `render_postscript` with the program as `source`, and `width` and
   `height` for the page, then **look at the PNG it returns**. Really look:
   composition, color, balance, whether the idea reads. Looking is not
   showing: that preview is visible to you, not to the user, and a picture
   inside a tool call usually does not reach them. The hosted tools write no
   file, so when they asked to see the piece, say what you drew and give the
   program. The `pscat` skill's "Show the result to the user" has the details.
3. Refine and re-render. Small seeds, big differences: `N srand` at
   the top makes every run reproducible (same seed, same picture).
   `review_sheet` compares twelve seeds in one call (one entry per seed, with
   the seed set at the top of each program), or different programs, or one at
   several sizes (a 48 px icon beside its 512 px original), with labels (see the
   `pscat` skill's `references/sheets.md`).
4. `validate_postscript` runs a program without returning an image and reports
   errors and lint findings: use it before an expensive render, or to find
   where a long program fails.
<!-- cloud: sub "`pscat --capabilities`
(MCP: `describe_art_capabilities`) prints the real, structured inventory" -->
`describe_art_capabilities` returns the real, structured inventory
<!-- cloud: sub "documented in each file's header and in
`--capabilities`:" -->
documented in each file's header and in
`describe_art_capabilities`:
<!-- cloud: sub " If it is still slow, `pscat --bands N --png out.png piece.ps`
renders on N threads and gives the same PNG bytes, but only when the program
reads `/BandBBox [llx lly urx ury]` (user space; also `/BandIndex`,
`/BandCount`) and skips the rows outside it, plus a margin of two:" -->
 The server has no `bands` option and each render has a time limit. The
`/BandBBox where { ... } if` line in this program is a no-op here (the key does
not exist, so every row is painted): keep it, since it does no harm and the
program then also runs under `bands` elsewhere. The loop's cost is what to keep
down:
<!-- cloud: replace "Render it with `pscat --page 400x400 --png cloud.png cloud.ps`" -->
Render it with `render_postscript` (`width` 400, `height` 400). An older renderer has
no noise operators: probe `systemdict /fbm2 known`, and fall back to artkit's
`noise2`. For Ghostscript, `lib/noiseref.ps` has the same
noise as `nrfbm2` and friends. A worked example, `gallery/basin_and_range.ps`,
and the detail in `docs/guides/artkit.md` are in the
[repository](https://github.com/PSCatLabs/pscat).
<!-- cloud: sub "- A bare install, which is what plugin users have, has all of the
  standard 35" -->
- A server with no font packs, the safe assumption, has all of the
  standard 35
<!-- cloud: sub "come from `pscat fonts install --pack extras`; Korean/Japanese/Thai
  from `--pack cjk-thai`. `pscat --fonts` lists what this install can
  reach." -->
come from the `extras` font pack; Korean/Japanese/Thai
  from `cjk-thai`. `describe_art_capabilities` (kind `font`) and
  `get_renderer_info` say which packs this server has.
<!-- cloud: drop "## With an image model: paint once, compose in pscat" -->
<!-- cloud: sub "
Adding a piece to the gallery is a contributor task: see `gallery/README.md`
in a checkout." -->
<!-- cloud: sub "A `--sweep-seed` also seeds
  noise, unless the program calls `noiseseed`." -->
To compare seeds, set `N noiseseed` as well as `N srand`: native noise keeps its
  own seed table.
<!-- cloud: sub "`pscat: warning: font … is in the … font pack` on stderr;" -->
`pscat: warning: font … is in the … font pack` in the reply;
<!-- cloud: sub "so check `--fonts`." -->
so check `describe_art_capabilities`.
<!-- cloud: sub "render just that region at `--page` scale." -->
render just that region, with a smaller `width` and `height`.
<!-- cloud: sub "`pscat --lint` reports it as `[shadowed-operator]`." -->
`validate_postscript` reports it as `[shadowed-operator]`.
