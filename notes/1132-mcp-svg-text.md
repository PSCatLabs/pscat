## `svg_text` on the MCP server's render tools (issue #1132, 2026-11-19)

#1128 (#1119) added `--svg-text` to the CLI, library and npm, but the plugin's
own MCP server (`src/mcp_server.rs`) had no way to ask for it, so SVG from the
plugin always drew every glyph as an outline.

**What was built.**
- `render_postscript` and `start_render` take `svg_text` (boolean, default
  false), named to match the argument pscat-mcp (the hosted server) uses, so
  the two agree. It lives
  in the shared render schema and `RenderPlan`, so both tools get it from one
  place.
- It is applied only when `format` is `svg`: `render_run` then passes
  `--svg-text` to the child. The CLI exits on `--svg-text` without `--svg`, so
  png and pdf must never get the flag; there the argument is accepted and
  ignored (the same way `halftone` is png-only). A non-boolean is refused as
  `invalid_argument` whatever the format, so a typo does not pass silently.
- Without it nothing changes: the child gets the same arguments as before.
- Documented in `plugin/skills/pscat/references/mcp.md` (SVG text), the
  "MCP server" section of `docs/guides/agents.md`, and `docs/guides/muse.md`
  (which said no MCP tool had it). The hosted-only skill copy under
  `plugin-chatgpt/cloud/` is a separate file for the hosted server and is
  untouched.

**Tests.** `tests/mcp.rs` "svg_text (#1132)": schema on exactly the two render
tools; type refusal for png and svg; on gives `<text` with the string and a
smaller file, off/false is byte-identical to omitting it, `start_render` gives
the same SVG, png ignores it.
