---
name: pscat-chatgpt-setup
description: Get the pscat PostScript renderer working in a ChatGPT session before any pscat render, when `pscat` is not on PATH, the pscat MCP tools (render_postscript, eval_postscript) are not available, or `pscat-mcp.sh --locate` cannot reach github.com. Installs the plugin's pinned pscat release through the GitHub connector's workflow-artifact download, checks its SHA-256, and runs a smoke render. Use it instead of Ghostscript, Python or an image model whenever the user asks for pscat.
---
<!-- Contributors: this skill ships only in the ChatGPT plugin ZIP (scripts/build_chatgpt_plugin.py). The build fills in every double-brace placeholder from plugin/pscat-release and the pinned release's SHA256SUMS; never write a version or a hash here by hand. -->

# pscat in ChatGPT: install it, then render with it

The other pscat skills (`pscat`, `psart`, `psviz`, `functional-postscript`)
assume a `pscat` command or the pscat MCP tools. In a ChatGPT session in the
cloud neither is there at first: ChatGPT on the web does not start the
plugin's local MCP server, and the sandbox often cannot reach github.com.
This skill gets the `pscat` command line working, so the other skills can use
it. Rendering then goes through the command line, not the MCP tools.

**Never substitute another renderer.** When the user asks for pscat, do not
render with Ghostscript, Python or an image model instead. If a step below
fails, stop and tell the user which prerequisite is missing (the table at
the end), in one or two plain sentences.

## 1. Is pscat already there?

```sh
pscat --version 2>/dev/null || ~/.cache/pscat/plugin/bin/pscat --version 2>/dev/null || ls -d "$HOME"/pscat/pscat-{{PSCAT_VERSION}}-*/ 2>/dev/null
```

If this prints `pscat {{PSCAT_VERSION}}`, skip to step 4. If it lists a
`~/pscat/pscat-{{PSCAT_VERSION}}-<target>/` directory, put it on `PATH`
(`export PATH="$HOME/pscat/pscat-{{PSCAT_VERSION}}-<target>:$PATH"`) and skip
to step 4. If it prints a different version, carry on: this plugin is pinned to
**{{PSCAT_RELEASE}}**.

If the shell can reach github.com, the plugin's own installer is simpler.
When `sh "${PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/pscat-mcp.sh" --locate`
prints a directory, put that directory on `PATH` and skip to step 4. When it
fails (no network, no such file), use step 2.

## 2. Download the pinned release through the GitHub connector

This needs the **GitHub connector** with access to the public repository
`PSCatLabs/pscat`, and a shell in this session. The connector's ordinary file
and release reads cannot return a binary release asset; its **workflow
artifact download** (`download_workflow_artifact`) can. The release workflow
keeps a copy of each binary as a workflow artifact.

1. **Pick the target** from `uname -m` in the shell:
   `x86_64` → `x86_64-unknown-linux-musl`;
   `aarch64` or `arm64` → `aarch64-unknown-linux-musl`.
   Anything else: stop (no prebuilt pscat for it).
2. **Find the release run.** List the workflow runs of `release.yml` in
   `PSCatLabs/pscat` and take the most recent one whose **branch is
   `{{PSCAT_RELEASE}}`** (a tag push), whose conclusion is **success**, and
   which has an artifact named `binary-<target>`. A run of the same tag that
   only published has no binaries, so check the artifact list, not just the
   conclusion. Look the run and artifact ids up each time; never reuse ids
   from an earlier session.
3. **Download** the `binary-<target>` artifact with the connector's
   workflow-artifact download, into a directory the shell can read. The
   download is a ZIP holding one file,
   `pscat-{{PSCAT_VERSION}}-<target>.tar.gz` (about 14 MB).
4. If the artifact is missing or expired (workflow artifacts expire after
   about 90 days), stop and report it; see the table below.

## 3. Check it, then unpack it

Check the archive's SHA-256 **before** unpacking or running anything. The
expected values come from the release's `SHA256SUMS`:

| target | `pscat-{{PSCAT_VERSION}}-<target>.tar.gz` SHA-256 |
|---|---|
| `x86_64-unknown-linux-musl` | `{{SHA256:x86_64-unknown-linux-musl}}` |
| `aarch64-unknown-linux-musl` | `{{SHA256:aarch64-unknown-linux-musl}}` |

```sh
mkdir -p ~/pscat && cd ~/pscat
unzip -o /path/to/binary-<target>.zip      # the connector's download
sha256sum pscat-{{PSCAT_VERSION}}-<target>.tar.gz
```

Compare the printed hash with the table, character for character. If the
connector also shows the release asset's `digest` (`sha256:...`), it must be
the same value. **If they differ, delete the file, stop, and tell the user:
do not run it.**

Then unpack it, keeping the directory together (the binary finds its fonts
beside it), and run it:

```sh
tar -xzf pscat-{{PSCAT_VERSION}}-<target>.tar.gz
export PATH="$HOME/pscat/pscat-{{PSCAT_VERSION}}-<target>:$PATH"
pscat --version                            # must print: pscat {{PSCAT_VERSION}}
```

Each new shell needs the `export PATH=...` line again, or call the binary by
its full path.

## 4. Smoke test, and show the result

```sh
cat > hello.ps <<'EOF'
%!PS
1 0 0 setrgbcolor 200 200 120 0 360 arc fill showpage
EOF
pscat --png hello.png --page 400x400 hello.ps && ls -l hello.png
```

A red circle on a white page means pscat works. Show the user the PNG itself
(attach the file or display it), not a description of it.

From here on follow the `pscat` skill (and `psart`, `psviz`,
`functional-postscript`) as usual, with these differences:

- Render with the command line (`pscat --png out.png file.ps`, `--svg`,
  `--pdf`). The MCP tools those skills mention are not connected in this
  session. Do not tell the user the plugin's MCP server is working.
- The binary has no live window: `pscat file.ps`, `-i` and `--spool` fail.
  Always name an output file.
- Show every finished render by attaching or displaying the file.

## When a prerequisite is missing

Say which one, plainly, and what would fix it. Do not fall back to another
renderer without the user's say-so.

| what failed | tell the user |
|---|---|
| No GitHub connector, or it cannot read `PSCatLabs/pscat` | Connect the GitHub connector in ChatGPT (it is a public repository), then ask again. |
| The connector has no workflow-artifact download | This connector cannot fetch binaries, so pscat cannot be installed in this session. The ChatGPT desktop app runs pscat on your computer instead. |
| No `binary-<target>` artifact, or it expired | The pinned release's build artifacts have expired. A newer pscat plugin is needed. |
| `uname -m` is not x86_64 or aarch64 | There is no prebuilt pscat for this machine type. |
| No shell, or the shell cannot run downloaded programs | This session cannot run programs, so pscat cannot run here. |
| SHA-256 mismatch | The download does not match the published release, so it was not run. |
| `pscat --version` prints another version | Name both versions; use it only if the user agrees. |
