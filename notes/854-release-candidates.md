# #854: release candidates publish a GitHub pre-release only

**Why.** The owner wants a non-developer tester to try the plugin before
0.7.0 is published anywhere permanent. The plugin launcher (#853) downloads
its binary from a pinned GitHub Release. Actions artifacts need a GitHub
login, so they won't do.

**What.** A tag `vX.Y.Z-rc.N`, on a commit whose `Cargo.toml` says
`X.Y.Z-rc.N`, runs the whole release pipeline with the same gates and the
same `release` environment approval, then stops at a GitHub pre-release.

- `scripts/release_stages.py`:
  - `VERSION_RE`, `parse_version` (semver precedence) and `is_prerelease`;
  - `version-info VERSION`, which prints `prerelease=` and `stages=` for
    `GITHUB_OUTPUT`;
  - `crates`, `npm` and `registry` return `disabled` for an rc;
  - the draft is created and published with `prerelease=true` and
    `make_latest=false`;
  - a release whose prerelease flag doesn't match halts;
  - rc release notes;
  - `rehearse_prerelease`.
- `release.yml`:
  - `verify` outputs `prerelease` and `stages`;
  - the plan and publish steps use `stages`;
  - the crates.io auth, npm setup and mcp-publisher steps are skipped for
    an rc.
- `release-check.yml` accepts `X.Y.Z-rc.N` and skips `install-crate` for it.
- `release.sh` accepts `X.Y.Z-rc.N`, compares versions by semver precedence,
  writes no CHANGELOG section for an rc (cliff.toml's `tag_pattern` already
  ignores rc tags), and opens a lighter rc PR.

**Design calls.**

- **The rc version goes in `Cargo.toml`.** That is the cargo-release
  convention, and it keeps the existing invariants: the tag equals `v` plus
  the Cargo version, archive names carry the version, and the bundle check
  matches `pscat --version`. The alternative was to build the rc from a
  `0.7.0` Cargo.toml. That would ship binaries that report 0.7.0, under the
  final release's file names but with different bytes.
- **The approval stays.** The rc publishes public, attested binaries, and
  the attestations need the `release` environment's OIDC token anyway.
- **The pin check and the semver check stay blocking.** The tester needs
  `pscat fonts install`. Pre-1.0, `0.6.3` to `0.7.0-rc.1` is a minor bump.
- **`check_workflows.py` is unchanged.** The rc reuses the one `publish`
  job, so `id-token: write` still appears only there.

**Verified.**

- `python3 scripts/test_release_stages.py`: the rc class covers ordering,
  every stage asked for, never-latest, flag halts, the rehearsal, the CLI,
  and `release.sh` rc then release.
- A `workflow_dispatch` dry run of `release.yml` on a scratch commit at
  `0.7.0-rc.1`, covering verify, binaries and assemble (see the PR).
- `draft` and `publish` cannot run in a dry run. Their GitHub flags are
  covered by the fake backend, and the first live proof is cutting
  `v0.7.0-rc.1`.
