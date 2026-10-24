# Handwriting worded as a style, not as passing for real (#686, 2026-10-23)

The plugin directory's policy and the owner's intent agree: pscat's handwriting is
a set of hand-drawn styles, and nobody should use an agent to forge anyone's hand.
The owner decided to fix this in the wording only; the feature and the
sloppy-but-legible guidance are untouched.

What changed:

- **Wording.** "Handwritten" and "human-looking" became "handwriting-style" and
  "a natural, hand-drawn script" in the `pscat` skill (front matter and body),
  `plugin/README.md`, the `handwrite` tool's title and description in
  `src/mcp_server.rs`, `docs/guides/agents.md` and `docs/guides/handwriting.md`.
- **One purpose statement**, in the skill, `plugin/README.md`, the tool description and
  both guides: hand-drawn styles for notes, cards and art; not for imitating a specific
  person's handwriting or presenting writing as someone else's.
- **A decline rule** in the skill body: for a named or real person's hand, or a document
  meant to pass as written by someone, decline that part and offer a generic hand.
- **Tool count** (from #688): `plugin/README.md` and `README.md` said four MCP tools;
  the server has five (`chalkboard_math`). The skills already listed five.

Left alone on purpose: the Design intent paragraph in `docs/guides/handwriting.md`,
the `lib/handscript.ps` header and `docs/dev/TASTE.md` (the glyph guidance the owner said to
keep exactly); both `plugin.json` manifests (no handwriting claim in them beyond the
`handwriting` keyword); `server.json` (a CODEOWNERS path), `site/`, the CLI help text and the
generated CHANGELOG.
