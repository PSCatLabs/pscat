# MCP tool annotations (#687)

Every `pscat-mcp` tool now advertises the five MCP annotation hints
(`title`, `readOnlyHint`, `destructiveHint`, `idempotentHint`,
`openWorldHint`). Needed for OpenAI's directory review (#682 section C) and
used by local clients to decide when to confirm a call.

**What changed**
- `src/mcp_server.rs`: an `annotations(title, read_only)` helper, called for
  each tool in `tool_list`. All fields are explicit (the spec's defaults are
  the pessimistic ones). The top-level `title` stays beside `annotations.title`.
- Descriptions say what the hints can't: `eval_postscript` and the render tools
  run a program that can read local files; the writers write only with
  `out_path`.
- `tests/mcp.rs` `every_tool_has_annotations`: a per-tool table; a tool with no
  row fails, so #688's `chalkboard_math` has to declare its hints.
- `docs/guides/agents.md` "Tool annotations", `docs/dev/HANDOFF.md`.

**Values**: read-only for `describe_art_capabilities` and `eval_postscript`;
not read-only for `render_postscript` and `handwrite`. Not destructive,
idempotent, closed-world everywhere.

**Judgment calls**
- `destructiveHint: false` on the writers follows the issue table and #682's
  owner checklist. Tension: the spec reads false as "only additive updates",
  and `out_path` replaces an existing file with the render's extension. We
  keep false because the write is confined to the output root, can't touch
  anything but a same-extension regular file, and goes through temp file plus
  rename. If a directory reviewer disagrees, flip the boolean in `annotations()`.
- `idempotentHint: true` on all four, including the read-only tools, where the
  spec ignores it. A seeded render is deterministic; a repeated write replaces
  the same file. (A program that reads the clock is not byte-identical, but the
  environment effect is the same.)
- The hints are static per tool, hence `readOnlyHint: false` for the render
  tools even when no `out_path` is passed.
