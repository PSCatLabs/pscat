# 1089: pscat in ChatGPT through the GitHub connector's artifact download

Refs #1057 (pscat in ChatGPT agents), #682 (directory listings), #689 (hosted
MCP server).

## The problem

The plugin's renderer is a local (stdio) MCP server. ChatGPT on the web does
not start it, and the session's shell often cannot reach github.com, so the
launcher's `--locate` download fails and there is no Cargo. The agent then
used Ghostscript (#1057).

## The route (proved in a ChatGPT session, 2026-11-08)

The GitHub connector's ordinary reads return release metadata but not a binary
release asset. Its workflow-artifact download (`download_workflow_artifact`)
does return a file. The release workflow uploads each target's archive as an
artifact named `binary-<target>` (`.github/workflows/release.yml`, the
binaries job), so the session found the `v0.7.0-rc.4` release run
(41963316156), downloaded `binary-x86_64-unknown-linux-musl`, checked it,
ran `pscat --version` (`pscat 0.7.0-rc.4`) and rendered with the CLI.

Checked from here on 2026-11-08:

- The artifact is a ZIP holding one file, `pscat-0.7.0-rc.4-x86_64-unknown-linux-musl.tar.gz`,
  whose SHA-256 (`9637b09a…`) equals both its `SHA256SUMS` line and the
  release asset's `digest` field in the REST API. So the artifact is the
  released archive, and a hash check against the release covers it.
- The release notes do not list the hashes, and a connector may not return
  the asset `digest` field. So the build writes the expected SHA-256 of each
  Linux target into the skill (from `--sums`, the pinned release's
  `SHA256SUMS`). It cannot be committed: the pin moves in the release PR,
  before that release's `SHA256SUMS` exists.
- The run's `event` is `push` and its `head_branch` is the tag name, so the
  skill matches the run by branch = tag. A later publish-only run of the same
  tag builds no binaries, so the skill also requires the `binary-<target>`
  artifact.
- All the run's artifacts expire 2027-02-03, 90 days after the run.

## What shipped

- `plugin-chatgpt/skills/pscat-chatgpt-setup/SKILL.md`: the ChatGPT-only
  skill. Check for pscat, try `--locate`, else find the run, download the
  artifact, check its SHA-256 before unpacking, check `pscat --version`,
  render a red circle and show it. A table maps each missing prerequisite to
  what to tell the user. It never names a version or hash by hand: the build
  fills `{{PSCAT_RELEASE}}`, `{{PSCAT_VERSION}}` and `{{SHA256:<target>}}`.
- `scripts/build_chatgpt_plugin.py`: the ZIP (Agent Plugins 1.0, `plugin.json`
  at the root) from tracked files only. It leaves out `hooks/` (they do not run
  in ChatGPT in the cloud, and a public-submission ZIP may not hold hooks),
  `README.md` and Claude Code's `.claude-plugin/plugin.json`. `--skills-only`
  also leaves out `mcp.json`. The ZIP is reproducible. The shared skills are
  copied byte for byte, which the tests check.
- `scripts/plugin_check.py` `check_chatgpt`: OpenAI's interface limits
  (`displayName` and `shortDescription` at most 30 characters, which made
  `shortDescription` "PostScript to PNG, SVG and PDF"; `defaultPrompt`; a
  square PNG icon of 48 to 4096 pixels), and the plugin skill checks on
  `plugin-chatgpt/` plus its placeholder rules.
- `scripts/test_build_chatgpt_plugin.py`, run by `plugin.yml`.

Why a separate skill and not a section in the shared `pscat` skill: the owner
asked for the fallback in ChatGPT installs only, and the shared skills stay
identical across Claude Code, Codex, Cowork and ChatGPT.

## Sources (read 2026-11-08)

- <https://developers.openai.com/plugins/deploy/desktop-only>: "Desktop-only
  plugins require local execution on the user's computer. This includes most
  plugins with a local (stdio) MCP server", installable only from the desktop
  app, usable "only in threads that run locally".
- <https://learn.chatgpt.com/docs/enterprise/plugin-management>: imported
  plugins can get the Desktop only label "when they declare MCP servers,
  including servers with remote HTTPS URLs" (this disagrees with the page
  above; the trial settles it).
- <https://developers.openai.com/plugins/deploy/submission>: the interface
  field limits; "Plugin ZIP packages containing app references ... or
  lifecycle hooks cannot currently be submitted"; the limits are for public
  submission, and "a package upload can accept longer draft text".
- <https://learn.chatgpt.com/docs/plugins>: "Plugin hooks are not supported in
  cloud-orchestrated ChatGPT Work."
- Not documented anywhere read: the private ZIP upload's steps, its accepted
  formats and layout, and whether a personal (non-workspace) account can use it.

## Not checked yet (needs a ChatGPT session)

1. The ZIP from this build installs as a private plugin; which label it gets
   with and without `mcp.json`.
2. ChatGPT lists five skills, and loads `pscat-chatgpt-setup` before a render
   when pscat is missing. If it does not, the fallback is a one-line pointer
   added to the ZIP's copy of `pscat/SKILL.md` at build time, with a test
   that it is the only difference.
3. The skill's steps work as written in a fresh session, and the failure
   table's messages appear when the connector is missing.

## Follow-ups (proposed)

- A scheduled workflow that re-uploads the pinned release's Linux archives as
  a fresh artifact, so the route outlives the 90-day expiry. The skill checks
  the hash against the release, so the copy needs no trust of its own.
- Attach the ZIP to each release (`release.yml`; it changes the archive count
  check and `SHA256SUMS`), as its own owner-merged PR.
- The hosted HTTPS MCP server (#689) stays the route to the MCP tools in
  ChatGPT on the web.
