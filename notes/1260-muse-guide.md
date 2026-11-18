# #1260: install guide for Muse

Added `docs/guides/muse.md` from the owner's Muse-written draft (2026-11-17),
with the corrections the issue lists. Evidence, all taken 2026-11-17.

## The rc.5 archive (`gh release download v0.7.0-rc.5`)

- Assets: five platform archives (`x86_64` and `aarch64` Linux musl, two macOS,
  Windows zip), `pscat-0.7.0-rc.5.tgz` (npm package), `SHA256SUMS`,
  `release-manifest.json`.
- `shasum -a 256` of the x86_64 musl archive is
  `bf7a94ee...d2d4`, equal to its `SHA256SUMS` line.
- It unpacks into one folder, `pscat-0.7.0-rc.5-x86_64-unknown-linux-musl/`,
  holding `pscat`, `pscat-mcp`, `lib/`, `fonts/` (licences), the licence files
  and a README. The draft's "extract to .../bin/" would have left the binaries
  one level down, so the guide's prompt says to strip the top folder.
- `pscat mcp` and `pscat-mcp` are the same server (`src/main.rs`,
  `src/mcp_server.rs`). A local rc.4 run of `pscat mcp` with protocol
  2025-11-25 listed the same 10 tools. The server finds its worker through
  `current_exe` (`src/mcp_server.rs`), so the two binaries stay together.
- `--out-root DIR` must exist; without it `out_path` is off when the working
  directory is `/` (`default_out_root`).

## The staging server (curl, Streamable HTTP, no auth)

- `initialize`: protocol 2025-11-25, `pscat-mcp` 0.1.0-rc.1.
- `tools/list`: 7 tools, all `readOnlyHint: true`.
- `get_renderer_info`: renderer pscat 0.7.0-rc.5; limits `render_timeout_ms`
  20000, `max_pixels` 16000000, `max_side_px` 8000, `max_source_bytes` 262144,
  `max_pages` 10, `max_output_bytes` 8388608, `max_concurrent_renders` 2,
  `svg_blob_threshold_bytes` 131072 (SVG comes back as a base64 blob when the
  pages together are over it); `jobs.enabled` false.
- `render_postscript` arguments: `source`, `format`, `dpi`, `width`, `height`,
  `halftone`.
- The in-session `pscat-render` tool is a different deployment (renderer rc.4,
  4 concurrent renders). Its numbers are not used.

## Other checks

- 0.6.3 (`git grep v0.6.3 -- src/mcp_server.rs`): four tools (render_postscript,
  handwrite, describe_art_capabilities, eval_postscript), no chalkboard_math, no
  `chalk` subcommand. `npm view pscat` and crates.io both give 0.6.3.
- Handwriting input is lowercased (`src/handwriting.rs`, `to_ascii_lowercase`);
  `docs/guides/handwriting.md` says the same.
- SVG text: `--svg-text` is a CLI flag (`docs/guides/getting-started.md`). The
  MCP `format` value maps only to `--svg` (`src/mcp_server.rs`), and the hosted
  `render_postscript` schema has no such argument.

- Handwriting text with no drawable character (`@@ é`): the hosted server returns
  an error, `Handwrite failed (invalid_request): text has no character this hand
  can draw: ...` (that check is in the hosted server, not this repo); the local
  `pscat handwrite` and the local MCP `handwrite` (rc.4) return a blank note.
- `validate_postscript` on `40 2 add =` (hosted) gives `valid: true`, output 42
  and a `blank-page` lint; local `eval_postscript` gives just `42`.

## Not verified (taken from the draft or the issue)

- That Muse has no plugin marketplace or MCP slot, that its workspace is Linux
  x86_64, where the files go (`~/workspace/custom-connectors/...`), and what Muse
  shows or saves after a render.
- The prompt changes against the tested draft (`--strip-components`,
  `--out-root`, fetching `SHA256SUMS` instead of a pasted hash): checked against
  the code and archive, not run in Muse.
- Hosted anonymous access ending 2026-11-19 17:00 UTC and the absence of public
  token issuing (from the issue).

## Release notes

The GitHub release body for a stable release is the version's `CHANGELOG.md`
section (`changelog_notes` in `scripts/release_stages.py`), which
`scripts/release.sh` writes with git-cliff from merged PR titles and the owner
polishes in the release PR. The announcement is drafted from that section
(`docs/dev/ANNOUNCING.md`). So the hook is a checklist line in
`docs/dev/RELEASING.md` (and the matching `release` issue template), plus this
PR's title, which git-cliff will list under Documentation. `release.sh` edits
no guide, so a second line covers moving the guides' rc pins at 0.7.0.
