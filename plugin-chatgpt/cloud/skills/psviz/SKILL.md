<!-- Contributors: edits the pscat-cloud ChatGPT plugin applies to plugin/skills/psviz/SKILL.md (scripts/build_chatgpt_plugin.py --remote, #1256); an anchor that no longer matches the shared text fails the build. The hosted server has seven tools, no command line and no local files. -->
<!-- cloud: sub "Both are compiled into `pscat`, so `(lib/dataviz.ps) run` works from any directory with an installed pscat (the `pscat` skill has the command line; over MCP, `render_postscript` returns the PNG)." -->
Both are built into the server, so `(lib/dataviz.ps) run` works in any program (render it with `render_postscript`; the `pscat` skill has the tool reference).
<!-- cloud: drop-item "- Ghostscript cannot see pscat's built-in library" -->
<!-- cloud: drop-item "- In the browser (the wasm build)" -->
<!-- cloud: replace-item "- When the user asked to see the figure, save the PNG" -->
- When the user asked to see the figure, say what it shows and give the program, since the hosted tools write no file. The `pscat` skill's "Show the result to the user" has the details.
<!-- cloud: replace-item "- `pscat --page 612x792 --png out.png file.ps` succeeds" -->
- `render_postscript` with `width` 612 and `height` 792 succeeds with no PostScript error (`validate_postscript` checks a program without an image).
<!-- cloud: sub "`pscat --capabilities` and the `lib/graph.ps`" -->
`describe_art_capabilities` and the `lib/graph.ps`
<!-- cloud: sub "run unchanged in Ghostscript when file access is permitted (see Pitfalls — SAFER) — sibling libs" -->
run unchanged in Ghostscript when file access is permitted — sibling libs
