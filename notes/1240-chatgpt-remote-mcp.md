# 1240: ChatGPT and the hosted (remote) pscat-mcp server

Part of #689. Related: #1089 (the plugin ZIP), #682, #1136.

## The question

The hosted pscat-mcp server (staging, Cloud Run) speaks streamable HTTP and
wants `Authorization: Bearer <token>`. Can ChatGPT (a) get it from a plugin ZIP
and (b) send that token?

## Answers (OpenAI documentation read 2026-11-16)

### (a) A plugin ZIP can declare a remote HTTP server: yes

<https://developers.openai.com/plugins/build/plugins> ("Bundled MCP servers
and lifecycle hooks"): "Configure portable MCP servers in root `mcp.json`.
Include the Agent Plugins MCP schema and a named entry under `mcpServers`."
Its remote example:

```json
{ "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
  "mcpServers": { "docs": { "type": "streamable-http", "url": "https://example.com/mcp" } } }
```

"For public submission, submit the remote HTTPS endpoint through **With MCP**."
<https://developers.openai.com/plugins/deploy/submission>: "only one MCP
server can be connected per plugin", so the remote ZIP replaces the local
`pscat` entry instead of adding to it. A ZIP that holds only a remote server is
not the "Desktop-only" kind (<https://developers.openai.com/plugins/deploy/desktop-only>:
"most plugins with a local (stdio) MCP server"), though
<https://learn.chatgpt.com/docs/enterprise/plugin-management> (see #1089)
says imported plugins can get that label "when they declare MCP servers,
including servers with remote HTTPS URLs". Unsettled until tried.

### (a') Without a ZIP: a custom MCP server by URL

<https://developers.openai.com/api/docs/guides/developer-mode> (the "Add custom
MCP server" guide): "Go to ChatGPT Plugins. Select the plus button, then **Add
custom MCP server**. Enter a name ... Under **Connection**, enter your
**Server URL** ... Configure authentication for your server ... Select
**Create as a plugin**." "Supported MCP protocols: SSE and streaming HTTP."
<https://developers.openai.com/plugins/deploy/connect-chatgpt>: enter "the MCP
server URL, including the `/mcp` path". The developer-mode guide adds
"Access: Use ChatGPT on the web."
<https://developers.openai.com/plugins/deploy/app-review> sends private use
here: "For private or workspace-only use, use a custom MCP server connection
instead." The steps for uploading a plugin ZIP to a personal account are still
undocumented (as in #1089); "Publish a local plugin to your workspace" (admin)
is documented in build/plugins.

### (b) Static bearer auth: not usable in ChatGPT

The manifest schema accepts it. <https://developers.openai.com/plugins/build/plugins>
("Fixed bearer tokens and API keys"): "A server that accepts a fixed bearer
token without an OAuth authorization flow uses API-key authentication. Its
manifest `auth` declaration is `{"type": "api_key", "headerScheme": "bearer"}`
... it doesn't contain the token." Then: "API-key declarations aren't
supported by the submission portal's connection form."

ChatGPT cannot send one:

- The custom-server form
  (<https://developers.openai.com/api/docs/guides/developer-mode>):
  "Authentication options include **OAuth**, **No authentication**, and
  **OAuth or no authentication**." No bearer, API-key or header field is
  mentioned.
- <https://developers.openai.com/plugins/deploy/submission> ("Authentication
  limitations"): "The portal connection form supports no authentication,
  OAuth, and mixed unauthenticated/OAuth access. Although the manifest schema
  accepts API-key authentication, the portal doesn't provide an API-key
  connection flow or a bearer-token input. This limitation also applies to
  Basic and custom-header API-key declarations. Uploading an API-key
  declaration alone won't complete MCP setup." Also: "Don't embed a token in
  server `headers` to work around this limitation: public submission rejects
  nonempty server headers. Don't configure a protected server as `type:
  "none"`."
- <https://developers.openai.com/plugins/build/auth>: ChatGPT "does **not**
  support machine-to-machine OAuth grants such as client credentials, service
  accounts, or JWT bearer assertions, nor can it present custom API keys or
  customer-provided mTLS certificates." The only bearer it sends is an OAuth
  access token ("`Authorization: Bearer <token>`") from its own OAuth flow.
- Secure MCP Tunnel (<https://developers.openai.com/api/docs/guides/secure-mcp-tunnels>)
  is for private servers; it uses the same authentication choices and adds a
  client to run, so it does not help a public Cloud Run server.

What was not tested: whether the Codex desktop app, which also reads
`mcp.json`, honours `api_key`. This work targets ChatGPT.

For a server that stays no-auth, OpenAI documents two ways to limit callers to
ChatGPT (build/auth): allow ChatGPT's published egress IP ranges
(<https://developers.openai.com/api/docs/guides/ip-addresses>), and verify the
OpenAI-managed client certificate presented on TLS (mTLS).

## What shipped

- `scripts/build_chatgpt_plugin.py --remote URL --remote-auth {none,bearer}`:
  the ZIP with its `mcp.json` replaced by one `streamable-http` entry named
  `pscat`; everything else is byte for byte the local ZIP (a repo test checks
  `mcp.json` is the only difference). Output `...-remote.zip`.
  - `--remote-auth` is required and there is no default: the staging server
    is protected, and OpenAI says not to configure a protected server as
    `none`. `bearer` writes the `api_key` declaration and prints a warning that
    ChatGPT cannot send it. There is no `oauth` choice: its settings depend on
    the owner's OAuth provider choice (registered client, DCR or CIMD).
  - The URL must be https with a host and a path, no userinfo, query or
    fragment (so a token cannot travel in it). It is an argument, so no
    staging or production host is committed.
  - Not with `--skills-only`.
  - Considered and rejected: a `headers` entry for the token (the docs above
    say public submission rejects it, and a token in a ZIP is a secret in an
    artifact).
- `docs/guides/chatgpt-hosted.md`: the custom-server setup steps, what the
  user sees, which skill steps do not work against the hosted tool set, the
  `--remote` ZIP, and the authentication choice.
- `scripts/test_build_chatgpt_plugin.py`: `Remote` tests and one repo test.

## Gaps

- The hosted server has 7 tools, the local one 10: no `eval_postscript`, no
  `start_render`, `render_status`, `render_result`, `cancel_render`; it adds
  `get_renderer_info` and `validate_postscript`. The shared skills
  (`plugin/skills/pscat/SKILL.md` line 285 on, `references/mcp.md`) describe the
  local set and are unchanged here.
- The `--remote` ZIP still ships `pscat-chatgpt-setup` (#1089), whose steps
  install a local pscat. Its description says to fire when the pscat MCP tools
  are not available, and step 1 checks for the `pscat` command, not the tools,
  so an agent might start the download although the hosted tools are
  connected. Untested; the skill is left alone here.
- Limiting a no-auth Cloud Run service to ChatGPT (paraphrased, from
  <https://docs.cloud.google.com/run/docs/securing/ingress>, which lists the
  ingress setting "Internal and Cloud Load Balancers" for an External
  Application Load Balancer and a "Disable the default URL" section, and
  from Google's Cloud Armor pages, which put an IP allowlist in a security
  policy on that load balancer): an allowlist is a Cloud Armor policy on an
  external Application Load Balancer, with ingress limited to it and the
  default `run.app` URL disabled so callers cannot go around it. It is not a
  setting of the service itself. An mTLS check was not researched (a load
  balancer or the server itself).
- Nothing here was tried in a ChatGPT session: the ZIP's label, whether a
  personal account can install it, and the connector flow on the real server.

## Follow-ups (proposed)

- Hosted-server note for the shared skills: a short paragraph in
  `plugin/skills/pscat/references/mcp.md` naming the tools the hosted server
  lacks and the 20 s limit (touches the skills every agent shares, so its own
  PR).
- After the owner's decision: OAuth on pscat-mcp (a pscat-mcp issue), or a
  separate no-auth staging service; then a `--remote-auth oauth` choice in the
  builder with the matching `auth` stanza.
- `pscat-chatgpt-setup`: step 1 should look for the pscat MCP tools first and
  stop when they are there (skill change, its own PR).
- Once a ChatGPT session has tried it: fix the "Desktop only" and personal ZIP
  upload statements in `docs/guides/chatgpt-hosted.md`.
