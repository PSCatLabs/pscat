# chalkboard_math's union schemas in Codex and Claude Code (#712)

**Question.** `chalkboard_math`'s `board` (bool, name, [r,g,b] or object) is
an `anyOf`, and `ghosting` (number or bool) was a `type` array. Do the
plugin's two hosts accept them?

**Method (real clients, not the documented subset).** Both CLIs were
installed, so both were run against `target/debug/pscat-mcp` (build `pscat`
too: the server runs it as a child).

- Codex 0.164.0: `codex exec --ephemeral --skip-git-repo-check -s read-only
  --json -c 'mcp_servers.pscat.command="<abs path>/pscat-mcp"' '<prompt>'
  </dev/null`. Redirect stdin: with a background shell it waits on stdin
  forever and prints nothing.
- Claude Code 2.1.309: `claude -p --strict-mcp-config --mcp-config <json>
  --allowedTools mcp__<server>__chalkboard_math --output-format stream-json
  --verbose '<prompt>' </dev/null`.

**Result.** Both list the tool and the model sent every form: `board`
`"slate"`, `"greenboard"`, `false`, `[0.1,0.2,0.3]`, `{"tone":"greenboard"}`;
`ghosting` `0`, `0.5`, `true`, `false`, absent. Every call returned a PNG.
Asked in plain words for "a slate board without ghosting", both sent
`board:"slate"` and a ghosting of `false`. Codex does not flatten the
unions: it showed the model `board?: boolean | "slate" | "greenboard" |
Array<number> | { [key: string]: unknown; }` and `ghosting?: number |
boolean` (it drops min/max, which the server checks anyway).

**What did change.** The MCP Inspector's `--strict` check warned that a
`type` array is "less portable": a client that maps schemas onto a
single-`type` dialect (Gemini's function declarations) may reject the tool
or drop the constraint. `ghosting` is now `anyOf: [number 0..1, boolean]`,
the inspector's own suggestion; the accepted values and the server's
behaviour are unchanged (the server never read the schema). The tool
description no longer says `&` aligns every multi-row input: `gather`,
`gather*` and `gathered` rows are centred since #699.

**Pinned.** `tests/mcp.rs::tool_schemas_use_only_the_portable_union_shape`
(no `type` array, `$ref`, `oneOf`, `allOf` or `not` in any tool's schema;
every `anyOf` branch has one `type`; the exact `board` and `ghosting`
branches; `review_sheet`'s dpi items; the gather wording).
`scripts/mcp_inspector_check.sh` now calls `chalkboard_math` with each
board form and `review_sheet` with a `[72, {"dpi": 9, "zoom": 2}]` list, and
fails on any `Warning:` from `--strict`.
