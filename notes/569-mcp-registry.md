## The MCP Registry entry (issue #569 part c, 2026-10-21)

Parts (a) and (b) shipped the Claude Code and Codex plugins. This registers
pscat in the official MCP Registry (registry.modelcontextprotocol.io), per
the resolved spec comment on #569 (2026-10-21).

### What shipped

- **`pscat mcp`.** The server moved from `src/bin/pscat_mcp.rs` to
  `src/mcp_server.rs`, which both binaries include with `#[path]`;
  `pscat-mcp` is now a three-line wrapper. Kept out of the library on
  purpose: it spawns processes and is no part of the crate's API. Its unit
  tests run once per binary (three small tests). `tests/mcp.rs` drives the
  subcommand through a handshake, `tools/list` and a tool call.
- **`server.json`** at the root: `io.github.PSCatLabs/pscat`, schema
  2025-12-11, one cargo package (`pscat`, stdio, positional argument
  `mcp`, following the registry docs' Snyk example). Written in
  `json.dump(indent=2)` layout, so `release.sh`'s bump rewrites only the
  two version lines (a test checks this).
- **README.md** carries `mcp-name: io.github.PSCatLabs/pscat` as a visible
  bullet under "For AI agents".
- **The `registry` release stage**, last in `STAGES`, with
  `RealRegistry`/`FakeRegistry`; `release.yml` installs `mcp-publisher`
  1.8.1 by SHA-256 in the `publish` job. RELEASING.md has the procedure.

### Decisions

- **Name casing.** Verified in the registry source (v1.8.1): the GitHub
  OIDC exchange grants `io.github.<repository_owner>/*`
  (`internal/api/handlers/v0/auth/github_oidc.go`), and permissions match
  by a case-sensitive prefix (`internal/auth/jwt.go`, `isResourceMatch`).
  Nothing on the publish path lowercases names. GitHub's API gives the
  owner login as `PSCatLabs`, and the live registry keeps mixed-case names
  (`io.github.GitHub30/...`). So `io.github.PSCatLabs/pscat`, exactly.
- **Login inside the stage**, right before publishing, not as a workflow
  step: an OIDC failure then fails only the last stage, after crates.io and
  npm, and a re-run retries only that.
- **Last stage.** After `publish` and `plugin`, so a registry outage never
  holds the GitHub Release in draft.
- **The README check runs twice.** Offline against the `.crate` (so the
  plan in `assemble` halts a release that would be unregistrable before
  anything is published), then against the README crates.io rendered,
  which is what the registry reads. crates.io renders it in a background
  job after a publish; `RealCrates.readme` polls for up to about 9 minutes.
- **Idempotency.** `GET /v0.1/servers/{name}/versions/{version}`: 404
  publishes; 200 compares name, version and each package's type,
  identifier, version, transport and arguments (the registry wraps the
  entry in its own metadata, so whole-document equality would not hold).
- **No `mcp-publisher validate` in CI.** It is not offline: it posts to the
  registry's `/v0/validate`. Ran it once by hand against this
  `server.json` (valid). The tests check the same structure offline.
- **mcp-publisher 1.8.1** (2026-08-06), the newest release, above the
  spec's 1.7.9 floor. The linux_amd64 digest was checked three ways: the
  GitHub API asset digest, `registry_1.8.1_checksums.txt`, and a local
  download.
- **mcpb deferred** to #678 (prebuilt bundles for users without Rust).

### Not verified

- A real publish: 0.6.3 is the first version whose README carries the
  token, so the first real run is that release. After it,
  `curl https://registry.modelcontextprotocol.io/v0.1/servers/io.github.PSCatLabs%2Fpscat/versions/0.6.3`
  should answer 200.
- The registry's OIDC check ignores the environment and the repository: any
  PSCatLabs repo's job with `id-token: write` could publish in the
  namespace. Here `check_workflows.py` confines OIDC to the `release` and
  `github-pages` environments.
