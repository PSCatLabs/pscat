# MCP pixel cap and server flags (#711), and the time limit docs (#741, part)

**What changed.** `pscat-mcp` has two new flags, `--max-pixels N` (default
40,000,000, at most 64,000,000) and `--max-latex-bytes N` (default 4096).
The pixel cap, which only `chalkboard_math` and `review_sheet` had (#688,
#708), now covers `render_postscript` and `handwrite` too, one figure for
every tool that allocates a canvas. Both were constants; they are `Config`
fields now. The reply is `output_too_large` with `limit_pixels`, `pixels`,
`width_px` and `height_px`.

**Why a flag on `pscat handwrite`.** The note's height is sized inside the
child from a line-count pass, so the server cannot know the area before the
run. The server checks the width (`width x dpi/72` over 8000 is refused at
once) and passes `pscat handwrite --max-pixels N`; the subcommand checks the
area after `auto_height` and before it draws, and prints
`output too large: WxH pixels (P) is over --max-pixels M`. That line is built
and parsed in one place, `pscat::limits::{pixel_cap_message,
parse_pixel_cap_message}` (the server and the CLI are separate binaries, so a
shared helper has to live in the library), and the server maps it back to
`output_too_large`.

**`render_postscript`** checks `width x height x dpi` before `ScratchDir` and
the spawn, for every format (SVG and PDF draw on the same raster). A program
cannot resize the page (no `setpagedevice`), so the pre-check cannot be
bypassed.

**#741, the part that is not async jobs.** The 60 s was presented as fixed in
the server instructions, the tool descriptions, SECURITY.md, the plugin README
and the pscat skill. It is the `--timeout` default (1..86400). Documented in
`docs/guides/agents.md` ("Raising the time limit"), with the traps found
checking it:
- `--max-steps` defaults to 25 billion, about seven minutes at the measured
  60 M steps/s, so `--timeout 3600` alone ends in `limit: "steps"`.
- Codex's `tool_timeout_sec` defaults to **60**, so under Codex the client,
  not the server, is the 60 s wall.
- Claude Code's `MCP_TOOL_TIMEOUT` (ms) defaults to about 28 hours; a stdio
  server that is silent for 30 minutes hits the idle timeout
  (`CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`, ms, or a per-server `timeout`), and
  `pscat-mcp` sends no progress notifications.
- The plugin's bundled server takes no options, so a raised limit means a
  hand-registered server and the plugin's one toggled off in `/mcp`.
Names and units come from the Claude Code and Codex docs (October 2026).
The async `start_render`/`render_status`/`render_result` part stays open in
#741.
