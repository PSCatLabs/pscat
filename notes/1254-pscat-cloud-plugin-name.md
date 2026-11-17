# 1254: the remote ChatGPT plugin is `pscat-cloud`

Owner test of the #1089/#1240 remote ZIP: ChatGPT refused the upload with "A
plugin named 'pscat' already exists", because the local plugin already holds
that name and `--remote` reused `plugin/plugin.json` unchanged.

## What changed

`scripts/build_chatgpt_plugin.py --remote` now writes a different plugin:

- `plugin.json`: `name` `pscat-cloud`, `displayName` "pscat cloud",
  `shortDescription` "Hosted PostScript renderer" (26 of OpenAI's 30
  characters), and a description saying it connects to the hosted `pscat-mcp`
  server and needs nothing installed (the local one says "Uses your installed
  pscat, or downloads the prebuilt one...").
- `mcp.json`: the one server is `pscat-cloud` (was `pscat`), so its tools do not
  collide with the local plugin's server.
- Output: `pscat-cloud-chatgpt-plugin-<version>.zip` (was
  `pscat-chatgpt-plugin-<version>-remote.zip`).

The owner chose `pscat-cloud` over `pscat-mcp`, which is also the local
server's name. The local and `--skills-only` ZIPs are byte for byte unchanged
(compared against the previous builder).

## Left in the remote ZIP on purpose

The local launcher scripts (`scripts/pscat-mcp*`) and `pscat-release` stay.
The shared `pscat` skill (`plugin/skills/pscat/SKILL.md`, lines 18 and 45) and
`pscat-chatgpt-setup` both tell an agent to run `scripts/pscat-mcp.sh --locate`
to fetch a local `pscat` command line, so they are referenced, not dead
weight. Dropping them would need those skills changed too. The setup skill's
description still says it fires when the pscat MCP tools are missing; that is
unchanged and untested against the hosted tools (see notes/1240).

## Other text updated

#1253 (publish the remote ZIP as a release asset) named the old file
`pscat-chatgpt-plugin-<version>-remote.zip` in its body; it now says
`pscat-cloud-chatgpt-plugin-<version>.zip`.
