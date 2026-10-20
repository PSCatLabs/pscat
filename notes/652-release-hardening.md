## Release hardening: --version probe, provenance, post-publish check, Dependabot (issues #652, #638, #636, #635, 2026-10-20)

Four small release-pipeline issues in one PR.

**#652, `--version` in the bundle probe.** `scripts/package_release.sh`'s
can-it-run probe is now `pscat --version` instead of `-e quit`. The probe and
the version comparison are separate checks. A binary that cannot start on a
cross target is still "archive checked, not run". A binary that runs but
prints anything other than `pscat X.Y.Z` (or `pscat-mcp X.Y.Z`) fails, even
on a cross target that happens to run (x86_64 macOS under Rosetta). The
outsider smoke test in RELEASING.md runs `pscat --version` first.

**#635, provenance.** The `publish` job (in the `release` environment, which
already has `id-token: write`) adds `attestations: write` and runs
`actions/attest-build-provenance` v4.2.2 (SHA-pinned) over the five archives
and the `.crate`, before the publish stages, so nothing goes public without
an attestation. A re-run attests the same digests again, which is harmless.
`scripts/check_workflows.py` needed no new rule: the OIDC token was already
confined to that job. Upstream says v4 is a thin wrapper over `actions/attest`
and new users should prefer that. The wrapper is still supported and is what
#635 named, so it stays; switching later is a one-line change. The attested
`.crate` is the one `verify` packaged. crates.io serves the one `cargo publish`
repackaged with the same toolchain, which is normally byte-identical, but the
crates stage also accepts a contents-equal crate. So the post-publish check
treats a crates.io `.crate` attestation mismatch as a warning.

**#638, the post-publish install check.** `.github/workflows/release-check.yml`
is a reusable workflow that release.yml calls after `publish`
(`post-publish-check`: `contents: read`, `attestations: read`, no environment,
no secrets). Two other triggers run it: `workflow_dispatch` for any published
version, and a pull request that edits the file, which tests it against
crates.io's newest version. That pull-request run is how the check itself gets
tested before a release depends on it.
- The archive job runs on Linux x86_64 and ARM64, macOS arm64 and Windows. It
  downloads the archive from the public URL and checks it against its own
  `SHA256SUMS` line. A `--ignore-missing` would pass an unlisted file. It runs
  `gh attestation verify --signer-workflow` when `attested`, then `--version`,
  a render and `handwrite`. All of it runs from an empty directory with an
  empty home. On Linux x86_64 it also runs `fonts install` and renders a pack
  face.
- The crate job runs on Linux and macOS. It waits a bounded 10 minutes for the
  sparse index, then runs `cargo install --locked`, `--version` and a render.
- 0.6.0 predates `--version`, so for it the check runs `-e quit` instead. The
  archive script was run locally against the published 0.6.0 on macOS arm64,
  including `fonts install`, and passed.
- release-check.yml joins `RELEASE_WORKFLOWS` in check_workflows.py: no
  workflow-level write, and per-job permissions everywhere. Tests cover a
  reusable-workflow call job.
- x86_64 macOS is not checked. GitHub has no Intel runners left, and Rosetta
  would only repeat the binaries job.

**#636, Dependabot.** `.github/dependabot.yml` sets up weekly updates, one
grouped PR for github-actions and one for cargo (`versioning-strategy:
lockfile-only`, so Cargo.toml requirements stay a deliberate decision). The
`msrv` CI job catches a lockfile bump that raises the effective MSRV.
Dependabot moves the SHA and its `# vN` comment together, so its PRs pass
check_workflows. `dtolnay/rust-toolchain` is pinned to a `master` commit,
which has no version tag, so Dependabot may propose a newer master commit or
nothing at all. That is fine either way. npm is left to #566.
`.github/dependabot.yml` is a CODEOWNERS path.
