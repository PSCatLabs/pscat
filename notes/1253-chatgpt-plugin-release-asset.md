# 1253: the hosted ChatGPT plugin ZIP as a release asset

What changed: every release, release candidates included, carries
`pscat-cloud-chatgpt-plugin-<version>.zip` (the `--remote` build of #1240/#1241,
with the cloud profile of #1256), listed in `SHA256SUMS` and
`release-manifest.json`, uploaded with the draft, and attested with the
archives.

Decisions:

- **The production URL is a repository variable, `PSCAT_MCP_PROD_URL`.** No URL
  is committed. The pscat-cloud production environment (pscat-cloud#26) had not
  published a URL when this was written. The `assemble` step reads the variable
  through `env`, prints the URL, and builds with the new
  `build_chatgpt_plugin.py --production`, which refuses an empty, non-https,
  `*.run.app` (staging) or placeholder (`example.com`, `.invalid`, ...) URL
  before anything is built. `release_stages.py manifest` then re-checks the
  finished ZIP's `mcp.json` (https, no `run.app`, plugin `pscat-cloud` of this
  version).
- **A dry run without the variable still rehearses the pipeline.** It builds
  against `https://pscat-mcp.example.com/mcp` and warns; that ZIP cannot reach a
  release, because `draft` and `publish` run only for a real tag. Without this,
  the pull request that edits `release.yml` (and every fork PR) would fail until
  the owner set the variable.
- **The ZIP is built into `release-inputs/`, not `dist/`.** `dist/` holds the
  binary archives, counted and enumerated by extension (`.tar.gz`, `.zip`);
  `manifest` copies the ZIP in afterwards, like the npm tarball (#1006), and
  `archives_in` skips the name prefix. The assemble count now matches
  `pscat-$VERSION-*`.
- **The builder needs a SHA256SUMS, which the manifest step writes.** The
  remote build reads only the two musl archives' digests from it and puts none
  in the ZIP, so the step builds from a provisional sums file over `dist/`.
- **Auth is `none`** (ChatGPT connectors accept no-auth or OAuth only,
  pscat-mcp#136); one word in the workflow step when that changes.
- **Rerun behaviour:** the ZIP is reproducible, so a re-run with the same URL
  matches an existing draft. Changing the variable after a draft exists makes
  `draft` halt on the differing asset (RELEASING.md).
- **`release-check.yml`** downloads the ZIP of releases from 0.7.1 on, checks it
  against `SHA256SUMS` and the attestation, and checks `mcp.json`.
- `docs/guides/muse.md` line 111 is the staging `*.run.app` address of the
  paused hosted preview: left alone.

Not done: the local-server plugin ZIP (`pscat-chatgpt-plugin-<version>.zip`).
The builder can make it (it needs the release's `SHA256SUMS` for the launcher's
checksums, which only exist after the manifest step), but it is "Desktop only"
in ChatGPT and the open question of #1089, so it is a follow-up.

Still open (owner): upload the ZIP of an rc to ChatGPT and render (#1089).
