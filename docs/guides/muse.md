# pscat in Muse

Tested by the project owner in Muse on 2026-11-17, with both routes below
working (what was and was not re-checked is at the end of this page). pscat is
a PostScript interpreter in Rust: it renders PostScript to PNG, SVG and PDF,
writes handwriting-style notes, and draws LaTeX formulas on a chalkboard. This
page covers 0.7.0-rc.5, a GitHub pre-release.

Muse has no plugin marketplace and no MCP slot you click to install pscat.
Instead you ask Muse to set up a small connector in its own workspace: paste
one of the prompts below and Muse does the setup itself. You do not need to
write any code.

Muse is not Meta AI. Meta AI (the meta.ai chat) has no MCP support, so none of
this works there.

Use the local install (Option A) if you can. The binary runs in Muse's
workspace and calls nothing on the pscat project's servers, so it is also the
one that keeps working. The hosted server (Option B) is paused: its
anonymous preview ended on 2026-11-18.

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
~/workspace/custom-connectors/custom.pscat/output/ first, then start the MCP
server over stdio with protocol 2025-11-25 as
<the absolute path of bin/pscat-mcp> --out-root <the absolute path of that
output folder> (use absolute paths, not ~ or a relative path: the server does
not expand ~, and you do not know which folder it starts in; the absolute path
of bin/pscat mcp starts the same server), run tools/list, and confirm it lists
10 tools. Test it with eval_postscript on "40 2 add =" (expect 42) and a small
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
- `40 2 add =` prints 42 (`eval_postscript` returns just `42`, with no
  warning), and a test render produces a PNG.

Notes on the prompt:

- The archive unpacks into one folder named after itself, holding `pscat`,
  `pscat-mcp`, `lib/`, the font licences and a short README. `pscat-mcp`
  starts the sibling `pscat` for every render, so the two binaries must stay in
  one folder. `lib/` is also compiled into the binary, so it keeps working if
  it moves.
- `pscat-mcp` and `pscat mcp` are the same server. Use whichever your client
  can launch.
- `--out-root DIR` is the one folder the tools may write files into. The
  `render_postscript`, `handwrite`, `chalkboard_math` and `review_sheet` tools
  take an optional `out_path` to save the result there; it is required only for
  a PDF from `render_postscript`, and for SVG or PDF from `chalkboard_math`.
  `DIR` must be the absolute path of an existing folder: the server does not
  expand `~`, and it refuses to start on a path that does not exist. Without
  `--out-root` the server uses the folder it was started in, and switches
  `out_path` off if that is `/`.
- Check the hash against the release's `SHA256SUMS`, not against a number
  copied into a guide or a chat, which can go stale.

<!-- rc-only: delete at 0.7.0 (docs/dev/RELEASING.md) -->
Do not use `cargo install pscat` or `npm install pscat` for this version. Both
still give 0.6.3, which has only 4 MCP tools, no `chalkboard_math` and no
`pscat chalk` command. 0.7.0-rc.5 is a GitHub pre-release only: it is not on
crates.io, npm or the MCP Registry.

## Option B: hosted server (paused)

**The hosted preview is paused.** Its anonymous access ended on 2026-11-18,
a day earlier than first announced. The staging server
(`https://pscat-mcp-145279231681.us-central1.run.app/mcp`) now accepts only
bearer tokens, and there is no public way to get one, so Muse's no-auth
connector gets `401 Unauthorized`. Use Option A.

The hosted route comes back when the server supports sign-in with OAuth
([pscat-cloud#15](https://github.com/PSCatLabs/pscat-cloud/issues/15)) or a
public server launches
([pscat-cloud#26](https://github.com/PSCatLabs/pscat-cloud/issues/26)). This
page will then have a new prompt. If you set up the hosted connector before
2026-11-18, remove it or switch it off, so Muse does not keep trying it.

Two things about the hosted server stay true when it returns:

- It has 7 tools, not 10: no `eval_postscript` (use `validate_postscript`),
  no render jobs, and no `out_path`. Pictures come back in the reply, or, if
  the client asks with `delivery`, as a short-lived download link.
- Its limits are tighter than the local server's: 20 seconds per render,
  16,000,000 pixels per page, 10 pages, and 8 MiB of output.

If Muse ever asks you for a token for a pscat server, never paste it into the
chat: give it to Muse only through its secure entry flow.

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
  emoji) draws nothing and the line moves along, so it leaves a gap. If none
  of the characters can be drawn, the hosted server refuses the call ("text has
  no character this hand can draw"), and the local one returns a blank note.
- **Text in SVG is drawn as shapes by default.** Every glyph is an outline, so
  the SVG looks the same everywhere but the text cannot be selected or
  searched, and the file is larger. The command-line option `--svg-text`
  (Helvetica, Times and Courier become `<text>`; see
  [getting started](getting-started.md)) is not an argument of the MCP tools,
  local or hosted, so Muse cannot turn it on through them. Run `bin/pscat
  --svg-text --svg ...` from the shell if you need it.

## Troubleshooting

<!-- rc-only: delete the next bullet at 0.7.0 (docs/dev/RELEASING.md) -->
- **Muse reports 0.6.3, or only 4 tools.** You have the old stable release.
  Install 0.7.0-rc.5 from the GitHub release in Option A, not from cargo or
  npm.
- **The hosted server answers `401 Unauthorized`.** The anonymous preview
  ended on 2026-11-18 (see Option B). Use Option A.
- **The binary will not run ("cannot execute", "exec format error").** The
  archive does not match the workspace's platform. Ask Muse to run `uname -sm`
  and pick the matching row of the Option A table.
- **`out_path is unavailable`.** The server was started in `/`. Start it with
  `--out-root` set to an existing folder, as in the prompt.
- **A render fails with a PostScript error.** Ask Muse for the exact error and
  the line it names. Common causes are a `curveto` with fewer than 6 numbers,
  and a `translate` that puts the picture off the page.
- **Handwriting comes out blank or with gaps, or the hosted server says "text
  has no character this hand can draw".** Characters outside a to z, 0 to 9 and
  `. , - ' ! ?` are skipped (see above). Capital letters are fine.

## What was and was not checked

The tool lists, limits and versions above were read from the release archive and
from the staging server on 2026-11-17 (`notes/1260-muse-guide.md` has the
details). What Muse itself does with them, such as how it stores a connector
or shows an image, is as the owner saw it on that day.

The local prompt adds three steps that the owner's tested prompt did not have.
They were checked against the release archive and the code, but not re-run in
Muse: stripping the archive's top folder, passing `--out-root`, and fetching
`SHA256SUMS` instead of comparing against a pasted hash.
