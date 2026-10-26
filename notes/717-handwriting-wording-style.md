# 717: handwriting wording aligned with #686's style framing

Wording only, no behaviour change. The surfaces that still said "handwritten"
now say "handwriting-style" (the CLI help, the subcommand list, the site, the
guides, `server.json`, the release bundle README), and the site's agents page
carries #686's purpose statement on the handwrite row. `site/agents.html`
links to the "Raising the time limit" section of `docs/guides/agents.md`.

`server.json` description: the MCP Registry caps `description` at 100
characters, so the registry line is shorter than the plugin and tool text. The
Registry copy updates only when a release publishes `server.json`.

Kept as is: the `handwrite` default output name (`handwritten.png`, a file
name, not wording), the font-face captions ("casual handwriting"), and the
`/HandScript` design intent.
