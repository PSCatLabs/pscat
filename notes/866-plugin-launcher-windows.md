# 866: the plugin launcher on Windows (one entry, two launchers)

Follow-up to #853 (PR #861). Since #861 both plugin manifests started the MCP
server as `sh <root>/scripts/pscat-mcp.sh`, and the hook as `sh ... --hook`.
Windows has no `sh` by default, so on Windows the plugin's server failed to
start. That was a regression for anyone who had `pscat-mcp` on PATH before.

## What the hosts allow (October 2026)

There is no platform condition in any of the three hosts.

- **Claude Code.**
  - <https://code.claude.com/docs/en/plugins-reference> lists every manifest
    field. None is per-OS, and an unknown top-level key is stripped.
  - Stdio MCP servers expand `${CLAUDE_PLUGIN_ROOT}` in `command`, `args`
    and `env`, and on Windows the substituted paths use forward slashes.
  - The hook `if` field is a permission rule, evaluated on tool events only
    (<https://code.claude.com/docs/en/hooks>).
  - How Claude Code spawns a stdio server is not documented. The claude
    2.1.310 binary bundles the MCP SDK's `StdioClientTransport`, which calls
    cross-spawn with `shell:false`. On Windows, cross-spawn looks a command
    up through PATHEXT and wraps a `.cmd` in `cmd.exe /d /s /c`.
  - Hooks: a shell-form hook runs in `sh -c` on macOS/Linux. On Windows it
    runs in Git Bash, or in PowerShell when Git Bash isn't installed (the
    `shell` field picks one explicitly). An exec-form hook (`args`) needs a
    real `.exe` on Windows: "The `.cmd` and `.bat` shims ... can't be spawned
    without a shell" (hooks page).
- **Cowork / claude.ai.**
  - <https://claude.com/docs/plugins/platform-support>: a local MCP server
    "Loads when the Cowork session runs on your computer", and hooks load.
  - <https://claude.com/docs/plugins/org-sync>: keep executables in
    `scripts/` and "reference them as `${CLAUDE_PLUGIN_ROOT}/scripts/<name>`
    from your skills, hooks, or MCP server configs". Running a plugin file
    directly is the documented pattern, which suggests the packaging keeps
    exec bits.
  - Neither page documents platform conditions or how Windows spawns the
    server.
- **Codex** (openai/codex at `19d7301`).
  - `codex-rs/codex-mcp/src/agent_plugin_config.rs`,
    `normalize_agent_plugin_stdio_server`: `command` must be "a bare
    executable name or a contained `./` path". It is resolved against the
    plugin root and gets no placeholders. There is no OS field.
  - `codex-rs/rmcp-client/src/program_resolver.rs`: on Windows, `resolve`
    calls `which::which_in(program, PATH, cwd)` so that `.cmd` and `.bat`
    run ("Windows cannot execute script files ... without their file
    extensions"). On Unix it returns the program unchanged.

So the one thing all three share is **name resolution**. On Windows, a path
with no extension resolves to its `.cmd` twin through PATHEXT. On Unix, the
same path is the file itself.

The prototype run on `windows-latest` (the first commit of PR #905) showed:

- `claude mcp list` (2.1.301) reported `plugin:pscat:pscat:
  D:/a/.../scripts/pscat-mcp - ✔ Connected`, through a `.cmd`-only test
  server.
- `codex mcp get` (0.163.0) gave the absolute extensionless path.
  `which::which_in` (which 8, Codex's version) on that path returned
  `...\scripts\pscat-mcp.cmd`, and running it worked.
- The hook string `cd "<root>/scripts"; ./pscat-mcp --hook` ran the `.cmd`
  in Windows PowerShell 5.1 and in pwsh, and the sh file in Git Bash.

## What changed

- **Entry point `plugin/scripts/pscat-mcp`** (sh, mode 100755). Both
  manifests name it: `${CLAUDE_PLUGIN_ROOT}/scripts/pscat-mcp` for Claude
  Code and `./scripts/pscat-mcp` for Codex, with no `args`.
  - On macOS and Linux it runs `exec sh pscat-mcp.sh "$@"`, so the
    launcher's behaviour there is unchanged and `pscat-mcp.sh` is
    byte-for-byte the same.
  - Under Git Bash, MSYS2 or Cygwin it hands over to the PowerShell twin.
- **`pscat-mcp.cmd`.** Windows hosts run this file. It calls
  `pscat-mcp.ps1 --exe`, which finds or installs pscat and prints the exe
  path, and then runs `pscat-mcp.exe` itself. That keeps PowerShell out of
  the MCP stdio path, and only `cmd.exe` stays resident. `--hook` and
  `--locate` go straight to the `.ps1`. It clears `PSModulePath`: one
  inherited from PowerShell 7 (a host started from pwsh) hid Windows
  PowerShell's own modules on the runner.
- **`pscat-mcp.ps1`**, for Windows PowerShell 5.1 and later. Its behaviour
  matches `pscat-mcp.sh`:
  - Lookup order: `PSCAT_MCP_BIN`, PATH (`Get-Command`, so an npm
    `pscat-mcp.cmd` counts), then `%CARGO_HOME%\bin` or
    `%USERPROFILE%\.cargo\bin`. A candidate must pass the version check,
    run with its stdin closed.
  - Then the cache, then a download of the one `-x86_64-pc-windows-msvc.zip`
    listed in `SHA256SUMS`. It checks the sha256 with .NET (`Get-FileHash`
    lives in the module that the stray `PSModulePath` hid) and unpacks with
    `ZipFile`.
  - **Atomic publish:** one `Directory.Move` of the unpacked bundle onto
    `<tag>\x86_64-pc-windows-msvc`. A move onto an existing name fails, so
    exactly one racer wins and a reader never sees half a tree. A failed
    move with no winner is retried for up to 10 s, in case an antivirus scan
    holds the new `.exe`.
  - **Lock:** a `FileShare.None` + `DeleteOnClose` file. Windows releases it
    when the process dies, so there is no stale-lock case.
  - **Offline vs missing:** an HTTP error (or a `file://` that names no
    file) gets the "is the release published" message. Any other failure
    is reported as offline.
  - **Offline fallback:** if the download fails, it uses the newest earlier
    download in the cache.
  - **Cache:** `%LOCALAPPDATA%\pscat\plugin`, overridden by
    `PSCAT_CACHE_DIR`.
  - **Arch:** x86_64 and ARM64 both get the x86_64 build (Windows 11 on ARM
    emulates it). 32-bit Windows has no prebuilt pscat.
  - **stdout:** `--hook` JSON and `--locate` go out as UTF-8 without a BOM;
    everything else goes to stderr.
- **Hook:** `cd "${CLAUDE_PLUGIN_ROOT}/scripts"; ./pscat-mcp --hook`. It is
  valid in sh, Git Bash, Windows PowerShell and pwsh: all of them have `cd`
  and `;`, and `./pscat-mcp` resolves to the `.cmd` in PowerShell. In
  Windows PowerShell 5.1, `&&` would be a parse error.
- **`.gitattributes`:** LF for the sh files and CRLF for `.cmd`/`.ps1`.
  Git for Windows defaults to `core.autocrlf=true`, and Claude Code clones
  the marketplace with the user's git, so without this a CRLF `pscat-mcp.sh`
  would fail under Git Bash.
- **`plugin_check.py`:**
  - A server `command` may be a plugin file (Codex's contained `./`, or
    Claude's `${CLAUDE_PLUGIN_ROOT}/`). That file must exist, have its exec
    bit, be committed as mode 100755 (checked with `git ls-files -s`), and
    an extensionless one needs a `.cmd` beside it.
  - The `cd ...; ./<file>` hook form is accepted, with the same checks.
  - The same-server rule treats `./` and `${CLAUDE_PLUGIN_ROOT}/` as the
    same root.
- **Tests:**
  - `test_plugin_launcher.py` adds the entry point's pass-through and the
    hook string from `hooks.json`, run under sh, dash and `bash --posix`.
  - `test_plugin_launcher_windows.py` has 16 tests on `windows-latest`,
    all in a temp directory whose name has a space and a non-ASCII letter
    (`pscat tést …`), as the Unix tests now are too.
    Each packs the CI-built `pscat.exe`/`pscat-mcp.exe` into a fake `.zip`
    release served from a local HTTP server, so a 404 and a refused
    connection are real outcomes. Each test speaks MCP to the real server
    through the `.cmd`. It covers:
    - cold and warm start, `--locate`, and the default `%LOCALAPPDATA%`
      cache;
    - eight parallel cold starts;
    - checksum mismatch, offline, missing release, and the offline
      fallback;
    - an installed copy that is new enough (PATH, `PSCAT_MCP_BIN`, Cargo
      bin), and an old one passed over;
    - the launcher named as `PSCAT_MCP_BIN` (no recursion);
    - the hook's three outputs, and the `hooks.json` string run in
      powershell.exe, pwsh and Git Bash;
    - the entry point under Git Bash;
    - a release-version pin.
  - `plugin_install_check_windows.py` installs the plugin with the pinned
    Claude Code and Codex CLIs on Windows, from a marketplace copy whose
    path has a space and a non-ASCII letter. Claude Code's `mcp list` checks
    three cases: pscat on PATH connects; not on PATH, it downloads from
    `file://` and connects; offline, it fails. For Codex, the resolved
    command's `.cmd` downloads and answers `tools/list`.
  - Both Unix install checks now assert the new files and the exec bit.

## Trade-off: the exec bit (owner decision)

`sh <script>` ran even from a copy that had lost its exec bit, which is why
#853 chose it. A single entry for every OS has to be the file itself, and
macOS and Linux then exec it directly.

- Claude Code (git clone) and Codex (copy) installs keep the mode. Both
  install checks print it, and they now fail if it is lost.
- `plugin_check.py` fails a stub committed without 100755.
- Cowork's packaging can't be tested from CI. Its docs do point at
  `${CLAUDE_PLUGIN_ROOT}/scripts/<name>` as the way to run a plugin
  executable.
- If a Cowork trial shows the bit lost, revert the two manifest lines to
  `sh .../pscat-mcp.sh`. That is a one-line change per manifest; Windows
  then loses the server again, but nothing else changes.

## Gaps and follow-ups

- The `.cmd` clears `PSModulePath`, so `pscat-mcp.exe` also starts without
  one. pscat-mcp does not use it.
- If a host force-kills `cmd.exe` without killing the tree, `pscat-mcp.exe`
  could be left running. That is untested. Claude Code ends the server's
  whole process tree on Windows (its MCP docs), and every test closes stdin,
  the normal shutdown path.

- **Codex hooks on Windows** are not exercised: a real Codex hook run needs
  the `/hooks` trust review (#654).
- `--exe` hands the server's path to the `.cmd` through `for /f`, which
  decodes with the console code page and cannot spell every user name. So
  `--exe` prints `VAR|REST`: the user-specific start of the path is the name
  of an environment variable (`PSCAT_CACHE_DIR`, `LOCALAPPDATA`,
  `CARGO_HOME`, `USERPROFILE`) that cmd already holds in Unicode, and only
  the ASCII rest is text; `call set` expands it. The tests' paths contain
  `Ł`, which code page 437 lacks (Codex review, round 1). A pscat-mcp on PATH
  outside those four directories, in a directory the code page cannot
  spell, is still passed as text and would fail.
- Managed machines where Group Policy forbids `-ExecutionPolicy Bypass`, or
  that run PowerShell in Constrained Language mode, stop the launcher. The
  README tells those users to install the release `.zip` and put it on PATH.
- Old pins' downloads are never pruned, as on Unix.
- No stable `bin\pscat` name on Windows (Unix has `$CACHE/bin/pscat` as a
  symlink, and symlinks need privileges on Windows). The hook names the
  versioned directory instead.

## For #904 (WASM fallback)

The "no prebuilt pscat for this platform" case is one branch in each
launcher: `TRIPLE` empty in `pscat-mcp.sh`'s `install` and its hook, and
`$Triple` empty in `pscat-mcp.ps1`'s `Install` and `Hook`. A WASM fallback
hooks in at those four places. On Windows the `.cmd` runs whatever path
`--exe` prints, so a fallback that prints a runner script's path needs no
change to the `.cmd`.
