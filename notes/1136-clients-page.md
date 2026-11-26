# #1136: one page for every AI client

`docs/guides/clients.md` is new. It is a chooser (a client table, "local or
cloud?", limits side by side), so `agents.md`, the 600-line reference for the
local server, stays as it is and links to it; the README, `plugin/README.md`,
`site/agents.html`, `site/llms.txt`, `docs/guides/README.md`, `muse.md` and
`chatgpt-hosted.md` link to it too. `agents.md` headings are unchanged
(`#raising-the-time-limit` and others are linked from the site and the plugin).

## Evidence (2026-11-25)

A Claude Code session with a claude.ai-account connector to the hosted server
(OAuth). `get_renderer_info` returned:

```json
{"artifact_links":{"backend":"gcs","default_delivery":"inline","quota_bytes":33554432,"threshold_bytes":1048576,"ttl_seconds":1800},
 "jobs":{"enabled":false,"max_running_jobs":1,"job_timeout_ms":600000,"tasks":false},
 "limits":{"default_page_points":[612,792],"max_concurrent_renders":2,"max_dpi":1200,"max_handwrite_bytes":65536,"max_latex_bytes":4096,"max_output_bytes":8388608,"max_pages":10,"max_pixels":16000000,"max_program_output_bytes":16384,"max_sheet_cell_pixels":16000000,"max_sheet_cells":32,"max_sheet_pixels":16000000,"max_sheet_source_bytes":262144,"max_side_px":8000,"max_source_bytes":262144,"min_dpi":9,"render_timeout_ms":20000,"sheet_timeout_ms":20000,"svg_blob_threshold_bytes":131072},
 "renderer":{"engine":"pscat","revision":"51a4b2e5c57e567353ab7643ab4f28b064b72eb6","version":"0.7.0"},
 "sandbox":{"server_dumpable":false,"worker_filter":"seccomp"},"server_version":"0.1.0-rc.2"}
```

Differences from the issue's draft: SVG becomes a blob above 131072 bytes
(128 KiB), not 40 KiB. The host of that connector was not visible from the
session, so "production" rests on the dispatcher's statement that
`https://mcp.pscat.dev/mcp` is live.

A `render_postscript` of a filled circle returned a 200x200 PNG
(`ok: true`, renderer 0.7.0). The claude.ai website and mobile app were not
opened.

## Not verified (left as such on the page)

- menu labels for adding a connector (claude.ai, ChatGPT, Developer mode);
- whether claude.ai needs a `dpi` hint;
- `claude mcp add --transport http ...` and the Codex `[mcp_servers]` entry
  with `codex mcp login`, against the production server;
- Muse with OAuth; ChatGPT (either route); Codex cloud tasks;
- sign-in beyond OAuth through WorkOS AuthKit.

Grok is "not yet investigated" (#1142).

## Contradictions fixed in other pages

`muse.md` (Option B said the hosted route is paused), `chatgpt-hosted.md` (the
`--remote-auth none` ZIP vs a server that uses OAuth), `plugin/README.md`
("neither has been tried", bearer staging) now carry a dated update pointing
at the client page.

## Follow-ups (not done here)

- pscat-mcp README: replace the `your-host.example` placeholder with the
  production URL and link back to `clients.md`. It is another repository.
- The ChatGPT release ZIP: `build_chatgpt_plugin.py --remote-auth` has no
  `oauth` choice; production needs one.
- `muse.md` still names 0.7.0-rc.5 throughout (rc-only blocks), now that 0.7.0
  is out.
