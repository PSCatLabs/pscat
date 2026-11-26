# pscat in ChatGPT through the hosted MCP server

ChatGPT on the web cannot start a local MCP server, so the pscat plugin's
`pscat-mcp` tools reach it only through a **remote** (HTTPS) server. This
page covers connecting ChatGPT to the hosted `pscat-mcp`, what you see, and
the one open question: how the server proves who is calling.

Status (November 2026): the staging server is protected by a static bearer
token, and **ChatGPT cannot send a static bearer token**, so it cannot connect
to staging. Releases from 0.7.1 on carry a plugin ZIP built against the
**production** server with authentication `none`, which ChatGPT can use (the
auth choice is on pscat-mcp#136; the earlier decision is on issue #1240).
Whether ChatGPT accepts the ZIP upload itself is still unverified (#1089).

**Update, 2026-11-25:** the production server is live at
`https://mcp.pscat.dev/mcp` and signs callers in with OAuth, not
no-authentication. The `--remote-auth none` ZIP above was built for the other
choice, so whether it connects to production is not yet verified. For the
custom-server route below, choose **OAuth** for authentication. The table of
every client's routes is [pscat in every AI client](clients.md).

## What ChatGPT accepts

Checked against OpenAI's documentation on 2026-11-16
(`notes/1240-chatgpt-remote-mcp.md` has the quotes and URLs):

| Question | Answer |
| --- | --- |
| Can a plugin ZIP declare a remote server? | Yes. Root `mcp.json` takes `{"type": "streamable-http", "url": "https://.../mcp"}` entries. |
| Can ChatGPT add a remote server without a ZIP? | Yes: ChatGPT Plugins, plus, **Add custom MCP server**, a name, the **Server URL**, then authentication. Protocols: SSE and streaming HTTP. |
| Authentication ChatGPT's form offers | **OAuth**, **No authentication**, **OAuth or no authentication**. |
| A static bearer token or API key | The manifest schema accepts `{"type": "api_key", "headerScheme": "bearer"}`, but OpenAI's docs say ChatGPT has no connection flow or token input for it and "cannot present custom API keys". |

## Three ways to connect

1. **Download the plugin ZIP from the release page (the primary path).**
   Every release, release candidates included, carries
   `pscat-cloud-chatgpt-plugin-<version>.zip`: the `--remote` build below,
   made by the release workflow against the **production** pscat-mcp server
   (#1253). Its authentication setting is `none`, because ChatGPT connectors
   accept only no authentication or OAuth (pscat-mcp#136). Releases from
   0.7.1 on have it; earlier ones do not.

   ```sh
   V=0.7.1    # the release you want, for example 0.7.1-rc.1 for a candidate
   gh release download "v$V" -R PSCatLabs/pscat \
       -p "pscat-cloud-chatgpt-plugin-$V.zip" -p SHA256SUMS
   shasum -a 256 -c SHA256SUMS --ignore-missing        # sha256sum on Linux
   unzip -p "pscat-cloud-chatgpt-plugin-$V.zip" mcp.json   # the URL it connects to
   ```

   Without `gh`, use the same two files from
   `https://github.com/PSCatLabs/pscat/releases/download/v<version>/`. The
   `mcp.json` URL must be an `https` address that is not a `*.run.app`
   address (that is the staging service); if it is, do not upload the ZIP.
   Then upload the ZIP as a plugin in ChatGPT (see the caveat under
   [The plugin ZIP, remote build](#the-plugin-zip-remote-build): the upload
   flow is unverified).
2. **Build the ZIP yourself (the fallback)**, when you want a different
   server URL or your release has no ZIP: [The plugin ZIP, remote
   build](#the-plugin-zip-remote-build) below.
3. **Skip the ZIP** and paste the server URL into ChatGPT's **Add custom MCP
   server**: the next section.

## Connect without a ZIP (the documented private route)

Use ChatGPT on the web. Workspace permissions and security restrictions
apply to custom MCP servers, and OpenAI marks the feature as elevated risk:
connect only a server you trust.

1. Open [ChatGPT Plugins](https://chatgpt.com/plugins), select the plus
   button, then **Add custom MCP server**.
2. Name: `pscat-cloud` (not `pscat`: that name is the local plugin's, and ChatGPT
   refuses a second plugin of the same name). Description: "Render PostScript to PNG, SVG and PDF."
3. Under **Connection**, enter the **Server URL**, with the `/mcp` path:
   `https://<your-pscat-mcp-host>/mcp`. The staging host is a Cloud Run
   `run.app` address and production differs: read the production URL from
   the `mcp.json` of the release ZIP (route 1), or ask the server's owner.
   Never paste a token into the URL.
4. Authentication: choose the method the server was set up for, **No
   authentication** or **OAuth** (the owner's decision; a bearer-protected
   server will fail here).
5. Read the risk warning, select **I understand and want to continue**, then
   **Create as a plugin**.
6. Find the resulting plugin in your personal plugins (or the workspace where
   you created it) and install it before using it.
7. In a new conversation, type `@` and pick `pscat-cloud`.

What you should see at step 6: seven tools, `chalkboard_math`,
`describe_art_capabilities`, `get_renderer_info`, `handwrite`,
`render_postscript`, `review_sheet` and `validate_postscript`. Tools without a
read-only annotation are treated as write actions, so expect a confirmation
prompt before the first render in a conversation (you can remember the
approval for that conversation). A first call after the server has been idle
can take a second or two longer (scale to zero).

Try: `Use the pscat-cloud app's render_postscript tool to draw a red circle on a
white page and show me the PNG.`

After the server changes its tools, use the plugin's details page in ChatGPT's
settings to refresh its tools (OpenAI's wording: "refresh apps to pull new
tools, descriptions, and server instructions").

## The skills with a hosted server

The local pscat plugin's skills describe the local server's ten tools and the
`pscat` command line. The hosted server has seven: five of those
(`chalkboard_math`, `describe_art_capabilities`, `handwrite`,
`render_postscript`, `review_sheet`) plus `get_renderer_info` and
`validate_postscript`. It limits renders to **2 at a time and 20 seconds each**.
What it does not have:

- `eval_postscript` (print what a program prints): use `validate_postscript`
  or a render instead; both return what the program printed.
- The long-render tools `start_render`, `render_status`, `render_result` and
  `cancel_render`, and any step that relies on a render running longer than
  20 seconds. Keep programs small, or render at a lower DPI.
- Anything that needs local files or the `pscat` command line.

The `--remote` ZIP's skills are written for exactly this (#1256): the `pscat`
skill names the seven tools and their arguments, tells the agent to report a
connection failure and stop (it never installs or runs a local pscat, and does
not substitute another renderer), and its reference files, and the `psart`
and `psviz` skills, point at the tools instead of the command line. The ZIP has
no `pscat-chatgpt-setup` skill, whose job is to install a local pscat. The
local ZIPs, the Claude Code plugin and the Codex plugin keep the shared wording
unchanged. If an agent with an older ZIP still names a tool the hosted server
lacks, tell it to use only the seven tools above.

## The plugin ZIP, remote build

The release workflow runs this build for you (route 1 above), with
`--production` and the URL from its `PSCAT_MCP_PROD_URL` repository variable
(`docs/dev/RELEASING.md`). Run it yourself for another URL or an older
release.

`scripts/build_chatgpt_plugin.py --remote` builds the plugin ZIP that
`plugin/README.md` describes, with its local `mcp.json` replaced by one entry
for the hosted server, the plugin renamed **`pscat-cloud`**, and the local
install machinery and wording left out (below). The new name matters: ChatGPT refuses
to install a plugin whose name is taken ("A plugin named 'pscat' already
exists"), so the local `pscat` plugin and this hosted one can both be installed
in one account (#1254):

```sh
gh release download "$(grep '^v' plugin/pscat-release)" -R PSCatLabs/pscat -p SHA256SUMS
python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS \
    --remote 'https://<your-pscat-mcp-host>/mcp' --remote-auth none
# dist/pscat-cloud-chatgpt-plugin-<version>.zip
```

- `--remote-auth` is required, with no default, because the server's
  protection decides what ChatGPT can do. `none` writes
  `"auth": {"type": "none"}`; `bearer` writes `{"type": "api_key",
  "headerScheme": "bearer"}`, which describes today's staging server
  truthfully but which ChatGPT cannot send (the build warns). There is no
  `oauth` choice yet: its settings (a registered client, or dynamic
  registration, and the endpoints) follow the owner's choice of provider.
- The URL must be `https`, with a host and a path (usually `/mcp`), and no
  credentials, query string or fragment. The URL is an argument and is never
  committed. No token goes in the ZIP, and no `headers` entry either (OpenAI's
  public submission rejects nonempty server headers).
- `--production` is the release workflow's switch: it refuses an empty, non-https,
  `run.app` (staging) or placeholder URL, so a release can never ship a ZIP that
  points at staging.
- It cannot be combined with `--skills-only`.
- What differs from the local ZIP: `plugin.json` has `name` `pscat-cloud`,
  display name "pscat cloud", short description "Hosted PostScript renderer"
  and a description that says it connects to the hosted `pscat-mcp` server and
  needs nothing installed; `mcp.json` names its one server `pscat-cloud`, so its
  tools do not collide with the local plugin's `pscat` server; the file is
  `pscat-cloud-chatgpt-plugin-<version>.zip`. The skill files that the overlay
  edits differ; every other file is the same.
- The cloud profile (#1256) leaves out `scripts/` (the local launchers),
  `pscat-release` (a binary version pin) and the `pscat-chatgpt-setup` skill, so
  the ZIP holds `plugin.json`, `mcp.json`, the icon and the four skills with
  their references. The shared skills' local-only wording (the install and
  `--locate` steps, the command-line examples, `out_path`, render jobs, local
  files) is rewritten at build time from `plugin-chatgpt/cloud/`: one overlay
  file per skill file, each edit naming the heading, paragraph or exact text it
  replaces in the shared source, so the guidance that is not local-only has one
  source. A shared sentence that an edit depended on, once reworded, fails the
  build and `scripts/test_build_chatgpt_plugin.py`; update the overlay then. The
  local and `--skills-only` ZIPs never read the overlay.

OpenAI documents the custom-server route above and publishing to a workspace
for plugins you have added, but not the steps for uploading a plugin ZIP to a
personal account. Treat the ZIP as unverified until it has been tried in a
ChatGPT session. Two open questions from #1089 apply: whether ChatGPT labels
a plugin with a remote server "Desktop only" (one OpenAI page says imported
plugins can be labelled so even for remote HTTPS servers), and whether a
personal account can use the upload at all.

## Which authentication

- **No authentication**: easiest, and what ChatGPT supports today. Anyone
  who has the URL can render on your server, which costs money and invites
  abuse. OpenAI documents two ways to limit callers to ChatGPT: allow only
  ChatGPT's [published egress IP ranges](https://developers.openai.com/api/docs/guides/ip-addresses),
  and check the OpenAI-managed client certificate on the TLS connection (mTLS).
  On Cloud Run neither is a setting of the service itself. Google's
  documentation puts an IP allowlist in a Cloud Armor policy on an external
  Application Load Balancer, with the service's ingress set to internal and
  load-balancer traffic and its default `run.app` URL disabled (otherwise
  callers go around the policy), so the server's address would change. The
  mTLS check would sit in the server or the load balancer; neither was
  checked here.
  Neither restricts it to you: OpenAI says an IP allowlist "identifies traffic
  from an OpenAI-operated network, not a specific user or workspace, and does
  not replace request authentication", so another ChatGPT user who has the URL
  can still reach it. They keep out other callers, not other ChatGPT users.
  The server's 2 concurrent renders and 20-second limit bound each request,
  not the total bill.
- **OAuth**: callers sign in, so a leaked URL is harmless. It means an
  authorization server in front of `pscat-mcp` (OAuth 2.1 with discovery), a
  client registration choice (client ID, dynamic registration or a client ID
  metadata document) and a registered callback URL.

See also [pscat for AI agents](agents.md) for the local server and the other
agents, and `plugin/README.md` for the ZIP that runs a local server.
