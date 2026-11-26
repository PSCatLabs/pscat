# pscat in every AI client

Find your client in the table, then read "Local or cloud?" if it offers both
routes. This is the one page that says where pscat works and how to connect it;
the other pages (the README, the site, the plugin README) link here instead of
repeating it.

pscat reaches an AI client by one of two routes:

- **Local**: the `pscat` command-line tool, the skills (plain Markdown guides
  in [`plugin/skills/`](../../plugin/skills/)) and the stdio MCP server
  `pscat-mcp`, which runs on your computer and has ten tools. The plugin installs
  all three in one step. Details: [pscat for AI agents](agents.md) and
  [`plugin/README.md`](../../plugin/README.md).
- **Cloud**: the hosted MCP server at `https://mcp.pscat.dev/mcp`. Nothing to
  install: you add it to the client as a remote server and sign in with OAuth
  (by WorkOS AuthKit, as the project owner set it up; anyone who signs in can use it). It has
  seven tools and tighter limits (see below).

## Which routes each client has

How to read the Status column: **works** means someone ran it and saw a render
(the date is in "What was and was not checked" at the end). **Documented** means
the steps are written and the plugin is checked in CI, but this page has not
watched a person use it in that client. **Not yet verified** means a step or label
here comes from the client's own documentation and has not been tried. Nothing
here is "coming": the cloud server is live.

| Client | Local route | Cloud route | Status |
|---|---|---|---|
| **Claude Code** (terminal, IDE) | Plugin: `/plugin marketplace add PSCatLabs/pscat`, then `/plugin install pscat@pscat`. It installs the skills and registers `pscat-mcp`, downloading pscat on first use if you have none. | `claude mcp add --transport http pscat-cloud https://mcp.pscat.dev/mcp`, then sign in from `/mcp`. Name it `pscat-cloud`: the plugin's local server is `pscat`. | Local: documented, plugin checked in CI on Linux and Windows. Cloud: works through a claude.ai-account connector (seen on 2026-11-25); the `claude mcp add` line is not yet verified. |
| **Claude desktop app** (Cowork) | Plugin: **Customize > Plugins**, **Add > Add marketplace**, `PSCatLabs/pscat`, then install **pscat**. Cowork runs the server on your computer. A Cowork session that runs in the cloud gets the skills and the command line, not the MCP tools (seen in the 0.7.0-rc.1 trial). | The same custom connector as claude.ai, below. | Local: documented. Cloud: as claude.ai. |
| **claude.ai** (website, mobile) | None for the tools: chat does not start local MCP servers. In a code-execution workspace the skill can fetch the command-line tool and render with it (`pscat-mcp.sh --locate`, [the skill's "No MCP, or a cloud sandbox"](../../plugin/skills/pscat/SKILL.md)). | A custom connector with the URL `https://mcp.pscat.dev/mcp`, signing in with OAuth. | Cloud: the connector works, but the website and mobile app were not tested (a claude.ai-account connector, authenticated by OAuth, answered `get_renderer_info` and returned a render in a Claude Code session, 2026-11-25). Menu labels for adding a connector: not yet verified. Whether claude.ai needs a `dpi` hint to show a sharp picture: not yet verified. Sandbox route: not yet verified. |
| **ChatGPT** (web, desktop) | A plugin ZIP can be built (`scripts/build_chatgpt_plugin.py`; the upload is not yet verified). Per OpenAI's documentation a stdio MCP server is "Desktop only", so the web and mobile app get the skills only, with a GitHub-connector fallback that installs the command-line tool ([#1089](https://github.com/PSCatLabs/pscat/issues/1089), [`plugin/README.md`](../../plugin/README.md#chatgpt-trial)). | The `pscat-cloud` plugin installs beside the local one. Connect it by URL ("Add custom MCP server", authentication OAuth, as OpenAI's documentation names them on 2026-11-16; labels not verified) or from the release ZIP: [pscat in ChatGPT through the hosted MCP server](chatgpt-hosted.md). | Not yet verified in either route. The release ZIP of the first release after [#1431](https://github.com/PSCatLabs/pscat/issues/1431) declares OAuth (`--remote-auth oauth`), matching the server; the 0.7.1 release candidates' ZIPs declared `none`. No ChatGPT session has uploaded one or signed in: see that page's status. Directory listing: [#682](https://github.com/PSCatLabs/pscat/issues/682). |
| **Codex** (CLI, IDE, app) | Plugin: `codex plugin marketplace add PSCatLabs/pscat`, then `codex plugin add pscat@pscat`. | An `[mcp_servers.pscat-cloud]` entry with `url = "https://mcp.pscat.dev/mcp"` in `config.toml`, then `codex mcp login pscat-cloud` for OAuth. | Local: documented, plugin checked in CI. Cloud: not yet verified, neither the entry nor the login step. |
| **Codex cloud tasks** | The command-line tool inside the task's sandbox, fetched with `--locate`. | As above, if the task can reach the server. | Not yet verified. |
| **Muse** | Ask Muse to install the prebuilt Linux binary and register `pscat-mcp` over stdio, from a prompt you paste: [pscat in Muse](muse.md). | A Muse custom connector with the hosted URL. | Local: **works** (2026-11-17). Cloud: worked on 2026-11-17 against the earlier no-authentication preview; not yet re-tested against the production server with OAuth. |
| **Grok** (xAI) | Not yet investigated: [#1142](https://github.com/PSCatLabs/pscat/issues/1142). | Not yet investigated: [#1142](https://github.com/PSCatLabs/pscat/issues/1142). | Not yet investigated. |
| **Meta AI** (meta.ai) | Not supported: no MCP support. Meta AI is not Muse. | Not supported. | Not supported. |
| **Any other MCP client** (OpenClaw, Hermes, ...) | Register `pscat-mcp` over stdio: `{ "mcpServers": { "pscat": { "command": "pscat-mcp", "args": ["--out-root", "/path/to/output"] } } }` | A remote server entry: Streamable HTTP, the URL above, OAuth. | Local: documented. Cloud: not yet verified for a client other than claude.ai. |

Two names look alike. In this repository `pscat-mcp` is the local stdio program
that ships with `pscat`. The hosted server's source repository
([PSCatLabs/pscat-mcp](https://github.com/PSCatLabs/pscat-mcp)) has the same
name and registers itself as `pscat-render`, so when you add the hosted server
give it a name of your own (the examples here use `pscat-cloud`) and keep the
local plugin's server, `pscat`, as it is.

## Local or cloud?

**If you can run a program on your computer, use local.** It is the whole
engine: no time cap you cannot lift, your own files and fonts, files written to
disk, and nothing leaves your machine. **If you cannot (claude.ai, ChatGPT on the
web, a phone) or you only want a quick picture, use cloud.** It is zero install
and sandboxed.

Use **local** when:

- a program reads your own files, libraries or fonts (`--allow-read`, the font
  catalog) or you want output written to disk (`out_path`);
- a render takes longer than 20 seconds, is larger than 16 megapixels, or wants
  the render jobs, bands or the GPU preview;
- you need an SVG or PDF over the cloud's size caps;
- nothing may leave your computer.

Use **cloud** when:

- you are on claude.ai, ChatGPT or a phone, or you cannot or will not install a
  Rust binary;
- the render is quick and self-contained, using only pscat's built-in `lib/`.

The limits, side by side. Local numbers are the server's defaults, which whoever
registers it can change ([limits and options](agents.md#limits-and-options)).
Cloud numbers were read from `get_renderer_info` on 2026-11-25 and are the
server's, not yours to change.

| | Local (`pscat-mcp`) | Cloud (hosted) |
|---|---|---|
| Time per call | 60 s by default, raisable ([how](agents.md#raising-the-time-limit)); jobs (`start_render`) up to an hour | 20 s per render; no jobs |
| Largest page | 40,000,000 pixels by default | 16,000,000 pixels, 8000 pixels a side, 10 pages |
| Program size | 1 MiB | 256 KiB |
| Reads | the working directory, `--allow-read` folders, pscat's data directories | only the built-in `lib/`: no font packs, no your-own-files |
| Writes | `out_path`, inside one output folder | none: the result comes back in the reply |
| SVG and PDF | `out_path`, or inline | inline: an SVG over 128 KiB (131072 bytes) comes back as a base64 file blob, up to 8 MiB of output in all |
| Concurrency | your computer | 2 renders at a time |
| Tools | 10 | 7: no `eval_postscript` (use `validate_postscript`), no `start_render`, `render_status`, `render_result`, `cancel_render` |
| Network | none for programs | none for programs |
| Privacy | stays on your computer | the program and the picture go through the hosted server. By default nothing is stored; if the client asks for a download link, the file is kept for 30 minutes |

**Both routes** run the same engine, so a program that uses only the built-in
`lib/` and the standard fonts gives the same picture when the versions match.
(The cloud has no font packs or files of yours, and the ChatGPT `pscat-cloud`
skills are reworded for it.) To check you have the same version, run `pscat --version`
locally and call `get_renderer_info` on the cloud server: each prints the pscat
version it runs.

## Which tools each route has

Local: `render_postscript`, `eval_postscript`, `handwrite`, `chalkboard_math`,
`review_sheet`, `describe_art_capabilities`, `start_render`, `render_status`,
`render_result`, `cancel_render`. Cloud: `render_postscript`, `validate_postscript`,
`handwrite`, `chalkboard_math`, `review_sheet`, `describe_art_capabilities`,
`get_renderer_info`. What each does is in the table in
[pscat for AI agents](agents.md#what-pscat-does-and-where-an-agent-finds-it); the
hosted server's own tool reference is in its
[README](https://github.com/PSCatLabs/pscat-mcp#tools). The cloud skills in the
ChatGPT `pscat-cloud` plugin are worded for the seven tools
([#1256](https://github.com/PSCatLabs/pscat/issues/1256)); the Claude Code and
Codex plugin's skills describe the local ten, so on the cloud route tell the
agent to use only the seven.

## What was and was not checked

Checked on 2026-11-25, in a Claude Code session that has the hosted server
as a connector from a claude.ai account, authenticated by OAuth: `get_renderer_info` (server 0.1.0-rc.2, renderer pscat 0.7.0,
the limits in the table above) and a small `render_postscript`, which returned a
PNG. That is the whole of the first-hand evidence for the cloud route: the claude.ai website and mobile app themselves were not opened. The raw
output is in `notes/1136-clients-page.md`.

Taken from other pages, not re-run here: the local plugin commands (from
[pscat for AI agents](agents.md) and `plugin/README.md`, which the plugin CI
exercises with real Claude Code and Codex on Linux and Windows), the Muse
results ([pscat in Muse](muse.md), 2026-11-17), and what ChatGPT accepts
([pscat in ChatGPT](chatgpt-hosted.md), read from OpenAI's documentation on
2026-11-16).

Not verified, so this page does not say: the exact menu labels for adding a
connector in claude.ai and ChatGPT (including ChatGPT's Developer mode), whether
claude.ai needs a `dpi` hint, the `claude mcp add` and Codex `[mcp_servers]` lines
against the production server, how a client behaves when its OAuth sign-in
expires, and who can sign up and how (this page knows only that sign-in is OAuth through WorkOS AuthKit).

## Related

[#689](https://github.com/PSCatLabs/pscat/issues/689) (the hosted server),
[#682](https://github.com/PSCatLabs/pscat/issues/682) (directory listings),
[#1055](https://github.com/PSCatLabs/pscat/issues/1055) (a preview that only the
model sees), [#1083](https://github.com/PSCatLabs/pscat/issues/1083) (the
cloud-sandbox note in the skill), [#1142](https://github.com/PSCatLabs/pscat/issues/1142)
(Grok). Which tool to pick for a picture at all (pscat, an image model, or an
LLM's own SVG): [When to use pscat](when-to-use-pscat.md).
