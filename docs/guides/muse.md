# pscat in Muse

Tested by the project owner in Muse on 2026-11-17, with both routes below
working. pscat is a PostScript interpreter in Rust: it renders PostScript to
PNG, SVG and PDF, writes handwriting-style notes, and draws LaTeX formulas on a
chalkboard. This page covers 0.7.0-rc.5, a GitHub pre-release.

Muse has no plugin marketplace and no MCP slot you click to install pscat.
Instead you ask Muse to set up a small connector in its own workspace: paste
one of the prompts below and Muse does the setup itself. You do not need to
write any code.

Muse is not Meta AI. Meta AI (the meta.ai chat) has no MCP support, so none of
this works there.

Use the local install (Option A) if you can. The binary runs in Muse's
workspace and calls nothing on the pscat project's servers, so it is also the
one that keeps working. Use the hosted server (Option B) only while it is
available and the local install does not work for you.

For the other clients (Claude Code, Codex, ChatGPT) see
[pscat for AI agents](agents.md) and
[pscat in ChatGPT through the hosted MCP server](chatgpt-hosted.md).

## Option A: local install (recommended)

What you need: Muse, with a workspace on a machine the prebuilt binary runs on.
No Rust toolchain is needed. The release has these archives (all in
<https://github.com/PSCatLabs/pscat/releases/tag/v0.7.0-rc.5>):

| Platform | Archive |
| --- | --- |
| Linux, x86_64 | `pscat-0.7.0-rc.5-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 | `pscat-0.7.0-rc.5-aarch64-unknown-linux-musl.tar.gz` |
| macOS, Apple silicon | `pscat-0.7.0-rc.5-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `pscat-0.7.0-rc.5-x86_64-apple-darwin.tar.gz` |
| Windows, x86_64 | `pscat-0.7.0-rc.5-x86_64-pc-windows-msvc.zip` |

The prompt below names the Linux x86_64 archive, which is what Muse's workspace
used in the owner's test. If Muse's workspace is something else (ask it to run
`uname -sm`), replace the archive name with the matching row of the table, or
ask Muse to pick the matching asset itself.

Copy and paste this into Muse:

```text
Install pscat 0.7.0-rc.5 locally for me as a custom MCP connector. Download the
prebuilt Linux x86_64 release from
https://github.com/PSCatLabs/pscat/releases/download/v0.7.0-rc.5/pscat-0.7.0-rc.5-x86_64-unknown-linux-musl.tar.gz
and the SHA256SUMS file from the same release
(https://github.com/PSCatLabs/pscat/releases/download/v0.7.0-rc.5/SHA256SUMS).
Compare the archive's SHA-256 with its line in SHA256SUMS, and stop if they
differ. Extract the archive into ~/workspace/custom-connectors/custom.pscat/bin/
without its top-level folder (tar -xzf <archive> --strip-components=1 -C <bin>),
so that pscat, pscat-mcp and lib/ sit together in bin/. Keep them together.
Confirm bin/pscat --version says 0.7.0-rc.5. Create
~/workspace/custom-connectors/custom.pscat/output/, then start the MCP server
over stdio with protocol 2025-11-25 as
bin/pscat-mcp --out-root ~/workspace/custom-connectors/custom.pscat/output
(bin/pscat mcp is the same server), run tools/list, and confirm it lists 10
tools. Test it with eval_postscript on "40 2 add =" (expect 42) and a small
render_postscript. Save the connector details in
~/workspace/custom-connectors/custom.pscat/CONNECTOR.md and save pscat as a
reusable skill so a later session can find it.
```

What Muse should report back:

- `pscat --version` says 0.7.0-rc.5, and the SHA-256 matched the release's
  `SHA256SUMS`.
- `tools/list` returns 10 tools: `render_postscript`, `eval_postscript`,
  `handwrite`, `chalkboard_math`, `review_sheet`, `describe_art_capabilities`,
  `start_render`, `render_status`, `render_result`, `cancel_render`.
- `40 2 add =` prints 42, and a test render produces a PNG.

Notes on the prompt:

- The archive unpacks into one folder named after itself, holding `pscat`,
  `pscat-mcp`, `lib/`, the font licences and a short README. `pscat-mcp`
  starts the sibling `pscat` for every render, so the two binaries must stay in
  one folder. `lib/` is also compiled into the binary, so it keeps working if
  it moves.
- `pscat-mcp` and `pscat mcp` are the same server. Use whichever your client
  can launch.
- `--out-root DIR` is where the tools may write a file (the `out_path`
  argument: PDF output, and SVG or PDF from `chalkboard_math`). It must be an
  existing folder. Without it the server uses the folder it was started in, and
  switches `out_path` off if that is `/`.
- Check the hash against the release's `SHA256SUMS`, not against a number
  copied into a guide or a chat, which can go stale. For this release the
  Linux x86_64 line starts `bf7a94ee` and ends `d2d4`.

<!-- rc-only: delete at 0.7.0 (docs/dev/RELEASING.md) -->
Do not use `cargo install pscat` or `npm install pscat` for this version. Both
still give 0.6.3, which has only 4 MCP tools, no `chalkboard_math` and no
`pscat chalk` command. 0.7.0-rc.5 is a GitHub pre-release only: it is not on
crates.io, npm or the MCP Registry.

## Option B: hosted server

No download and no binary to maintain. The server is at
`https://pscat-mcp-145279231681.us-central1.run.app/mcp`. The transport is
Streamable HTTP, MCP protocol 2025-11-25. It is pscat-mcp 0.1.0-rc.1 running
the same pscat 0.7.0-rc.5 renderer as the local install, so the same program
gives the same picture.

**This is a staging preview, and its anonymous access ends on 2026-11-19 at
17:00 UTC.** After that time the server no longer accepts anonymous
requests, and there is no way for the public to get a token. A public hosted
server is tracked in
[PSCatLabs/pscat-cloud#26](https://github.com/PSCatLabs/pscat-cloud/issues/26).
Until it launches, use Option A.

Copy and paste this into Muse:

```text
Connect Muse to the hosted pscat MCP server at
https://pscat-mcp-145279231681.us-central1.run.app/mcp using Streamable HTTP,
with no auth. Run initialize with protocol 2025-11-25, then tools/list, and
confirm the tools. Test it with get_renderer_info, then validate_postscript on
"40 2 add =" (expect 42), then a small render_postscript. Save the connector
details in ~/workspace/custom-connectors/custom.pscat-cloud/CONNECTOR.md,
including the endpoint, the tool list, and the limits you find.
```

What Muse should report back:

- Server pscat-mcp 0.1.0-rc.1, renderer pscat 0.7.0-rc.5.
- `tools/list` returns 7 tools: `chalkboard_math`, `describe_art_capabilities`,
  `get_renderer_info`, `handwrite`, `render_postscript`, `review_sheet`,
  `validate_postscript`.
- `validate_postscript` on `40 2 add =` says valid, with program output 42.

### How the hosted server differs from the local one

- **No `eval_postscript`.** Use `validate_postscript` to check a program and
  see what it prints.
- **No render jobs.** `start_render`, `render_status`, `render_result` and
  `cancel_render` are local only.
- **Nothing is written to a file.** There is no `out_path`. Pictures come back
  inside the response, and Muse saves them for you.
  - PNG is an image block per page.
  - PDF is one embedded blob holding every page.
  - SVG is an embedded text resource per page. When all the pages together are
    over 131,072 bytes (128 KiB), they come back as base64 blob resources
    instead of text.
- **Tighter limits** (from `get_renderer_info`):

  | Limit | Value |
  | --- | --- |
  | Time per render | 20 seconds |
  | Pixels per page | 16,000,000 |
  | Longest side | 8000 px |
  | Program size | 262,144 bytes |
  | Pages | 10 |
  | Output | 8 MiB |
  | Renders at once | 2 |

  The local server's defaults are looser (60 s per call, 40,000,000 pixels,
  and long renders through `start_render`).
- **No files of your own.** Programs run in a sandbox: no file writes, no
  network, and reads only from pscat's built-in `lib/`.
- If a first call fails with a dropped connection, ask Muse to retry it. The
  server can be asleep, and the retry works.

### Authentication

No authentication is needed until 2026-11-19 17:00 UTC, as above. There is no
public way to get a token, so do not look for one. If Muse ever asks you for a
token for a pscat server, never paste it into the chat: give it to Muse only
through its secure entry flow.

## Using pscat once it is installed

Try these in Muse:

- "Draw a watercolor frog on a lily pad with pscat."
- "Write the quadratic formula on a chalkboard with pscat."
- "Write a short handwritten note with pscat."
- "Render this PostScript program as SVG with pscat."

Muse saves each render as a PNG, SVG or PDF file and shows it to you in chat,
with the PostScript source beside it so you can change it.

Two things worth knowing:

- **Handwriting is lowercased.** The handwriting font has no capital letters, so
  the text you give `handwrite` is lowercased first: "Hello, World" comes out
  as "hello, world". The font draws the letters a to z, the digits 0 to 9, and
  the marks `. , - ' ! ?`. Any other character (an at sign, an accent, an
  emoji) is not an error: it draws nothing and the line moves along, so it
  leaves a gap.
- **Text in SVG is drawn as shapes by default.** Every glyph is an outline, so
  the SVG looks the same everywhere but the text cannot be selected or
  searched, and the file is larger. The command-line option `--svg-text`
  (Helvetica, Times and Courier become `<text>`; see
  [getting started](getting-started.md)) is not an argument of the MCP tools,
  local or hosted, so Muse cannot turn it on through them. Run `bin/pscat
  --svg-text --svg ...` from the shell if you need it.

## Troubleshooting

- **Muse reports 0.6.3, or only 4 tools.** You have the old stable release.
  Install 0.7.0-rc.5 from the GitHub release in Option A, not from cargo or
  npm.
- **Muse cannot list tools on the hosted server.** Ask it to retry once, then
  check that the URL ends in `/mcp`. If it is after 2026-11-19 17:00 UTC, the
  anonymous preview has ended: use Option A.
- **The binary will not run ("cannot execute", "exec format error").** The
  archive does not match the workspace's platform. Ask Muse to run `uname -sm`
  and pick the matching row of the Option A table.
- **`out_path is unavailable`.** The server was started in `/`. Start it with
  `--out-root` set to an existing folder, as in the prompt.
- **A render fails with a PostScript error.** Ask Muse for the exact error and
  the line it names. Common causes are a `curveto` with fewer than 6 numbers,
  and a `translate` that puts the picture off the page.
- **Handwriting comes out blank or with gaps.** Characters outside a to z, 0 to
  9 and `. , - ' ! ?` are skipped (see above). Capital letters are fine.

## What was and was not checked

The tool lists, limits and versions above were read from the release archive and
from the staging server on 2026-11-17 (`notes/1260-muse-guide.md` has the
details). What Muse itself does with them, such as how it stores a connector
or shows an image, is as the owner saw it on that day.
