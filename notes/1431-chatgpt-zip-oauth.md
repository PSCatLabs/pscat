## ChatGPT cloud plugin ZIP: a `--remote-auth oauth` choice (issue #1431, 2026-11-26)

From #1136 (PR #1430). Part of #689; related #1089, #1240.

## The problem

The release ZIP was built with `--remote-auth none`, but production
(`https://mcp.pscat.dev/mcp`) answers 401 until the caller signs in with OAuth.
OpenAI's submission page says "Don't configure a protected server as `type:
"none"`", so the ZIP declared the wrong thing, and the builder had no way to
declare the right one.

## What was done

- `scripts/build_chatgpt_plugin.py --remote-auth oauth` writes
  `"auth": {"type": "oauth", "client": {"mode": "cimd"}}` into the ZIP's
  `mcp.json`. Nothing else: no client ID, no secret, no endpoint override, no
  host. `none` and `bearer` are unchanged.
- `.github/workflows/release.yml` (an owner path) now passes `--remote-auth
  oauth` for the production ZIP. Without it the bug would stay: the builder
  could declare OAuth but every release would still ship `none`.
- Docs say what is and is not known: `docs/guides/chatgpt-hosted.md`,
  `docs/guides/clients.md`, `docs/dev/RELEASING.md`, `plugin/README.md`.
- Tests: `scripts/test_build_chatgpt_plugin.py` pins the stanza, that the
  module table is not shared between builds, and that no choice carries a
  secret, token, client ID or `headers` entry.

## Why this shape

Quoted from OpenAI's "Package your plugin", section "Configure MCP
authentication" (<https://developers.openai.com/plugins/build/plugins.md>,
read 2026-11-26):

- "Declare authentication under each server's `extensions["com.openai"].auth`
  object in `mcp.json`."
- Method table: "OAuth | `auth.type: "oauth"`, with optional client
  registration settings".
- "To prefer Client Identifier Metadata Document (CIMD), use:
  `{"type": "oauth", "client": {"mode": "cimd"}}`".
- "Don't include `clientId` or client credentials in a `dcr` or `cimd` client
  object. CIMD preference doesn't disable the DCR fallback."
- Reference table: "`client` | Object | Optional client registration
  configuration." and "Omitted optional values remain available for
  discovery."

So `client` is optional, and a bare `{"type": "oauth"}` is also valid. CIMD
preference was chosen because it is OpenAI's stated preference, needs no
registration endpoint or stored client, and keeps DCR as the fallback. A
server that supports neither would fail either way.

What the server offers (probed 2026-11-26 with plain GETs, no credentials): its
protected-resource metadata names an AuthKit issuer, and that issuer's
metadata advertises `client_id_metadata_document_supported: true` and a
`registration_endpoint`, so both CIMD and DCR are on offer. The tenant host is
deliberately not in the script or the tests.

The JSON shape was checked against the OpenAI page above, not against a ChatGPT
session. The Agent Plugins `mcp.schema.json` (1.0.0) does not describe `auth`
(it is in the `com.openai` extension), so there is no published schema to
validate against.

## What shipped before

Release 0.7.1 (published 2026-11-26) carries
`pscat-cloud-chatgpt-plugin-0.7.1.zip`; its `mcp.json` points at
`https://mcp.pscat.dev/mcp` and declares `"auth": {"type": "none"}` (downloaded
and unzipped to check). The workflow change lands on `feature/0.8.0`, so a patch
release cut from `main` before that branch merges would still declare `none`
unless the change is backported.

## Not verified

- Whether ChatGPT accepts the ZIP upload at all (#1089), whether it labels the
  plugin "Desktop only", and whether sign-in then works and a tool call
  renders. The issue's "verify the ChatGPT route end to end" needs a ChatGPT
  session and is still open.
- OpenAI's OAuth plugin requirements for directory submission (a UserInfo
  endpoint that returns `email` and `email_verified: true`, and the `openid`
  and `email` scopes; <https://developers.openai.com/plugins/guides/submit-claude-plugin.md>)
  were not checked against the server. That is a #682 matter.

## Follow-ups (proposed)

- A guard in `release_stages.py check_chatgpt_plugin` that the production ZIP's
  auth is not `none`, so a regression to the old flag fails the release.
- Try the ZIP in a ChatGPT session (owner; #1089).
