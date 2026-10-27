# Releasing pscat

pscat has two release trains:

- **Interpreter releases**, `vX.Y.Z` tags (this document). Each one publishes
  the `pscat` crate to crates.io and a GitHub Release with binaries for five
  platforms, the `pscat` npm package (#566; first published with 0.6.2),
  checks the Claude Code plugin manifests, and registers the version in the
  MCP Registry (#569; from 0.6.3).
- **Font packs**, `fonts-YYYY.MM` tags. These are
  [`RELEASING-FONTS.md`](RELEASING-FONTS.md). An interpreter release pins one
  font-pack index, so the first font release comes before the first
  interpreter release.

After the one-time setup below, a release is: run `scripts/release.sh X.Y.Z`,
merge the PR it opens, push the tag, and approve the `release` environment.
The approval is the only manual step that repeats.

A **release candidate**, `vX.Y.Z-rc.N`, goes through the same steps but
publishes only a GitHub pre-release with the binaries, for testers to try
before anything reaches crates.io, npm or the MCP Registry. See
[Release candidates](#release-candidates).

## The pipeline

`scripts/release.sh X.Y.Z` opens the release PR. Run it from a clean,
up-to-date `main`. It refuses in these cases:

- the version is not newer than the current one (semver order:
  `0.6.3` < `0.7.0-rc.1` < `0.7.0-rc.2` < `0.7.0`);
- the tag `vX.Y.Z` already exists;
- `src/fontpacks/pinned.rs` has `PINNED_INDEX = None` (#616).

It creates `release/vX.Y.Z` and sets the version in `Cargo.toml`, in pscat's
`Cargo.lock` entry, and in every plugin, npm and MCP Registry manifest that
exists. It also sets the plugin launcher's release pin, `plugin/pscat-release`,
to `vX.Y.Z` (an rc gets `vX.Y.Z-rc.N`), so no release PR needs a hand edit
there (#867). The pin keeps its comment lines. It prepends the release's section to `CHANGELOG.md` with
[git-cliff](https://git-cliff.org) (`cliff.toml`), commits `release: X.Y.Z`,
pushes, and opens the PR with the checklist. Merging the PR publishes nothing.

Pushing the tag starts `.github/workflows/release.yml`. The reversible stages
run first:

| Job | What it does | Credentials |
|---|---|---|
| `verify` | Checks that the tag matches `Cargo.toml` and is on `main`. Checks that `PINNED_INDEX` is set and that its `font-index.json` hashes to the pinned SHA-256. Runs the quality gate, the MCP Inspector check, the licence checks, `scripts/check_package.sh` (crate acceptance), the release-script tests, and `cargo semver-checks` against crates.io once pscat is published. Generates the `cargo-about` notices and packages the `.crate` from the clean commit, then builds and packs the npm package (`scripts/build_npm.sh --pack release-inputs`). | read |
| `binaries` | Builds and checks one archive per platform (below) with `scripts/package_release.sh`. | read |
| `assemble` | Writes `SHA256SUMS` and `release-manifest.json` (version, tag, commit, every digest). Runs the rehearsal (below) on these exact artifacts. Runs a read-only plan against the real registries, which stops a real release on a mismatch before anything is created. | read |
| `draft` | Creates a **draft** GitHub Release with the archives, `SHA256SUMS` and `release-manifest.json`, uploading the manifest last. | `contents: write` |
| `publish` | Runs in the **`release` environment**, so it waits for the owner's approval. For a release candidate it runs only the GitHub pre-release and the plugin check ([Release candidates](#release-candidates)). It signs build-provenance attestations for every archive and the `.crate` (below), publishes the crate to crates.io (trusted publishing), then npm once `npm/package.json` exists (the exact tarball `verify` packed), then makes the GitHub Release public, then checks the plugin manifests once a `.claude-plugin/` manifest exists, then publishes `server.json` to the MCP Registry with the pinned `mcp-publisher`. | `contents: write`, `id-token: write`, `attestations: write` |
| `post-publish-check` | Calls `.github/workflows/release-check.yml`: installs the release from the public channels on Linux, macOS and Windows and fails if it does not install or run (below). | `contents: read`, `attestations: read` |

The draft exists before the approval prompt, so you can review it (assets,
notes, checksums) before you approve.

`draft` and `publish` run only for a pushed `v*` tag, and only while the
repository variable `RELEASE_ENABLED` is `true`. Everything else is a **dry
run** that stops after `assemble`:

- a pull request that touches the pipeline;
- a manual "Run workflow", with an optional `ref` input.

The `publish` job writes a release report (ref, commit, checksums, and each
stage's result) to the job summary and to the `release-report-<attempt>`
artifact.

### Safe resume

`scripts/release_stages.py` implements every publishing stage the same way.
It first asks the registry what is already there:

| The registry has | The stage |
|---|---|
| nothing, as a definite "not found" (HTTP 404, npm `E404`) | publishes |
| the same artifact | skips |
| something else, or something it cannot verify | **halts** (exit status 3) |
| no clear answer (network error, 5xx, auth failure) | fails (exit status 1); safe to retry |

"The same" means, per channel:

- **GitHub Release.** The release found for the tag has every asset, and
  each one's SHA-256 matches. If the release already carries a
  `release-manifest.json`, the assets are judged against it, after its tag,
  version and commit are checked against this release. A rerun whose
  rebuilt binaries differ bit for bit then adopts the draft instead of
  halting. Drafts are found by listing releases, because a get-by-tag
  lookup cannot see them. Two releases for one tag halt. The tag must point
  at the commit the artifacts were built from.
- **crates.io.** The API's checksum equals the SHA-256 of the `.crate`.
  Failing that, the published `.crate` has the same file contents and its
  `.cargo_vcs_info.json` names the release commit. A crate with no VCS info
  cannot be verified, so it halts.
- **npm.** The registry's `dist.integrity` equals the SHA-512 of the
  tarball `verify` packed (`release-inputs/pscat-X.Y.Z.tgz`, beside the
  `.crate`). The stage publishes that file, never a fresh `npm pack`, and
  halts if it is missing or holds another name or version.
- **MCP Registry.** The registry's entry for the name and version
  (`GET /v0.1/servers/{name}/versions/{version}`) lists the same packages
  as `server.json`: registry type, identifier, version, transport and
  arguments.

Nothing ever overwrites, deletes or re-uploads a published version. To
resume a failed release, use **Re-run failed jobs** on the same workflow run.
Finished stages skip; the rest run. The `publish` job asks for approval again
on each attempt.

#### When a stage halts

The job log and the report say which artifact differs and how.

- **A draft release differs or is incomplete in a way a rerun cannot fix.**
  Drafts are private, so look at it, delete the draft, and re-run.
- **A published GitHub Release differs.** Do not edit it. Find out where it
  came from. A release published by mistake is retired the way any release
  is (see [Yanking and deprecating](#yanking-and-deprecating)), and the fix
  ships as the next patch version.
- **crates.io holds a different `X.Y.Z`.** crates.io versions are permanent.
  Yank it if it is broken, and release `X.Y.Z+1`.
- **The MCP Registry stage halts.** Registry versions are immutable too. A
  different entry at the version, or a README without the `mcp-name:` line
  (crates.io serves each version's README as published, so that version
  can never be registered as a cargo package): fix it for `X.Y.Z+1`. The
  rest of the release is already out, because this stage runs last.
- **Two drafts for one tag.** Delete the extra one.
- **The tag points elsewhere.** Tags are immutable (see the tag ruleset
  below). Never move one. Release the next version.

## Platforms

| Archive | Built on | Notes |
|---|---|---|
| `aarch64-apple-darwin.tar.gz` | `macos-latest` (arm64) | |
| `x86_64-apple-darwin.tar.gz` | `macos-latest`, cross-compiled | GitHub is retiring its Intel macOS images. The bundle check runs the binary under Rosetta when the runner has it. Otherwise it checks the archive only and says so in the log. |
| `x86_64-unknown-linux-musl.tar.gz` | `ubuntu-latest` | Static and **headless** (`--no-default-features`) |
| `aarch64-unknown-linux-musl.tar.gz` | `ubuntu-24.04-arm` | Static and **headless** |
| `x86_64-pc-windows-msvc.zip` | `windows-latest` | |

**Why the Linux binaries are headless.** A fully static musl binary cannot
`dlopen` the system's X11 or Wayland libraries, which winit loads at run time,
so the live window could never open. The binaries are built without the
`window` feature instead. They still render to PNG, SVG and PDF and run every
subcommand, and the window modes print a clear error. A glibc build would keep
the window, but it ties the binary to the distribution's libc version. The
issue preferred musl. Linux users who want the window run `cargo install
pscat`. Revisit this if Linux window users ask for a prebuilt binary: add a
`x86_64-unknown-linux-gnu` row with the default features beside the musl one.

Each archive holds:

- the binaries and `lib/`;
- `LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-OFL` and `fonts/LICENSE`;
- `THIRD_PARTY_NOTICES.md` and `licenses/assets.json`;
- `THIRD_PARTY_LICENSES.html`, the `cargo-about` notices (`about.toml`,
  `about.hbs`) for every Rust dependency linked in;
- a README.

`about.toml` accepts the same licences as `deny.toml`, plus OFL-1.1. A
dependency under any other licence fails the build.

## Credentials and the trust boundary

- **No long-lived publishing secrets.** crates.io and npm use trusted
  publishing: inside the `publish` job, the job's OIDC token is exchanged for
  a short-lived registry token (`rust-lang/crates-io-auth-action`, and
  `npm publish --provenance`). The MCP Registry does the same with
  `mcp-publisher login github-oidc`, which needs no setup: the token's
  `repository_owner` claim (`PSCatLabs`, case and all) grants the
  `io.github.PSCatLabs/*` namespace. Unlike crates.io and npm, the registry
  does not check the environment or even the repository, so any job in a
  PSCatLabs repository with `id-token: write` could publish there; in this
  repository `check_workflows.py` keeps that to the `release` and
  `github-pages` environment jobs.
- **`mcp-publisher` is pinned.** `release.yml` names an exact release and
  the SHA-256 of its `linux_amd64` archive (`MCP_PUBLISHER_VERSION`,
  `MCP_PUBLISHER_SHA256`; 1.7.9 or later for CVE-2026-44428 and
  CVE-2026-45781). To move it, take the digest from that release's
  `registry_<version>_checksums.txt` and check it against a download.
- **Only the `release` environment can mint that token.** `id-token: write`
  appears in no other job, apart from the GitHub Pages deploy, which runs in
  its own `github-pages` environment. The crates.io and npm trusted publishers
  are configured to accept only `environment: release`, so even that token
  cannot publish. The environment admits only `v*` and `fonts-*` tags and
  needs the owner's approval. Pull-request CI never reaches it.
- **Actions are pinned to commit SHAs.** The tag each SHA stood for is
  written in a comment. Dependabot (`.github/dependabot.yml`, weekly, one
  grouped PR for actions and one for `Cargo.lock`) moves the SHA and the
  comment together; its PRs touch CODEOWNERS paths, so the owner merges
  them. To update a pin by hand, resolve the new tag with
  `gh api repos/OWNER/REPO/commits/TAG --jq .sha`. Never type a SHA from
  memory.
- **Signed provenance.** The `publish` job attests every archive and the
  `.crate` with `actions/attest-build-provenance` before anything goes
  public. The attestation is signed with the job's OIDC token, which is why
  it lives in the `release` environment job and nowhere else. See
  [Verifying a release](#verifying-a-release).
- **Every job declares least-privilege `permissions:`**, and the workflow
  default is `{}`.

`python3 scripts/check_workflows.py` enforces the pins, the OIDC rule, the
secrets rule and the per-job permissions on every workflow. It runs in CI's
`release-policy` job and in `verify`.

## One-time setup (owner)

These steps change repository settings, so an agent must not run them. Each
is a command for the owner, or a settings page where no API exists.

1. **The `release` environment.** The owner is the required reviewer.
   Self-review must be allowed: the owner both pushes the tag and approves it,
   and this is a single-maintainer repo. Admins cannot bypass the approval.
   Only `v*` and `fonts-*` tags may deploy.

   ```sh
   gh api -X PUT repos/PSCatLabs/pscat/environments/release --input - <<'EOF'
   {
     "reviewers": [{"type": "User", "id": 1081450}],
     "prevent_self_review": false,
     "can_admins_bypass": false,
     "deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}
   }
   EOF
   gh api -X POST repos/PSCatLabs/pscat/environments/release/deployment-branch-policies -f name='v*' -f type=tag
   gh api -X POST repos/PSCatLabs/pscat/environments/release/deployment-branch-policies -f name='fonts-*' -f type=tag
   ```

   (`1081450` is `gh api users/jeromebanks --jq .id`.) `release-fonts.yml`
   already runs its `publish` job in this environment.

2. **Immutable release tags.** This tag ruleset stops anyone from moving or
   deleting a `v*` or `fonts-*` tag. Only the owner (repository admin) can
   create them.

   ```sh
   gh api -X POST repos/PSCatLabs/pscat/rulesets --input - <<'EOF'
   {
     "name": "release tags",
     "target": "tag",
     "enforcement": "active",
     "conditions": {"ref_name": {"include": ["refs/tags/v*", "refs/tags/fonts-*"], "exclude": []}},
     "rules": [{"type": "deletion"}, {"type": "non_fast_forward"}, {"type": "update"}, {"type": "creation"}],
     "bypass_actors": [{"actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always"}]
   }
   EOF
   ```

   Optionally, turn on Settings → General → Releases → **Enable release
   immutability**. A published release's assets can then never change, which
   is what the pipeline assumes anyway.

3. **Required status checks.** These are the checks the #602 dispatcher note
   lists, plus `release-policy` from this change. They are the job names CI
   reports:

   ```sh
   gh api -X PATCH repos/PSCatLabs/pscat/branches/main/protection/required_status_checks --input - <<'EOF'
   {
     "strict": false,
     "contexts": [
       "test", "msrv", "packaging", "licences", "release-policy",
       "clean-install (macos-latest)", "clean-install (ubuntu-latest)", "clean-install (windows-latest)"
     ]
   }
   EOF
   ```

   Then set `required_status_checks` in `SDLC.md`'s frontmatter to the same
   list (re-running `sdlcify` reconciles the two). `deny`,
   `cross-platform (windows-latest)`, `crate (...)`, `docs` and `semver` stay
   informational.

4. **The first font release and its pin.** These are #616's prerequisites.
   Do them before the first interpreter release, as
   [`RELEASING-FONTS.md`](RELEASING-FONTS.md) describes:

   ```sh
   gh variable set FONTS_RELEASE_ENABLED --body true --repo PSCatLabs/pscat
   git switch main && git pull
   git tag -a fonts-2026.10 -m "font packs 2026.10" && git push origin fonts-2026.10
   # approve the release environment for the release-fonts run, then:
   gh release download fonts-2026.10 -p font-index.json
   python3 scripts/font_packs.py pin --index font-index.json
   # commit src/fontpacks/pinned.rs in a PR ("fonts: pin fonts-2026.10") and merge it
   ```

5. **Turn the interpreter pipeline on.**

   ```sh
   gh variable set RELEASE_ENABLED --body true --repo PSCatLabs/pscat
   ```

6. **crates.io bootstrap: the one documented exception to "no manual
   publishing".** crates.io can only attach a trusted publisher to a crate
   that already exists, so the first version is uploaded by hand. Do it
   during the first release run, after `draft` finishes and before you
   approve `publish`:

   ```sh
   git switch --detach v0.6.0            # exactly the tagged commit
   git status --porcelain                # must print nothing
   cargo +1.96.1 publish --locked        # RELEASE_TOOLCHAIN in release.yml; token below
   ```

   Create the token at <https://crates.io/settings/tokens> with the
   `publish-new` scope only, limited to `pscat`, with a short expiry. Pass it
   as `CARGO_REGISTRY_TOKEN` for that one command, then revoke the token.
   Then configure trusted publishing: crates.io → `pscat` → Settings →
   Trusted Publishing → Add → GitHub, with owner `PSCatLabs`, repository
   `pscat`, workflow `release.yml`, environment `release`.
   Now approve the run. The `crates` stage finds `0.6.0` already on crates.io
   with the same contents from the same commit, skips it, and the release
   continues. A manual publish from any other commit, or from a dirty tree,
   makes that stage halt, which is the point. (Use the workflow's
   `RELEASE_TOOLCHAIN` so the `.crate` bytes match `verify`'s exactly. Another
   cargo version still matches, through the contents-and-commit check.)

7. **npm bootstrap: the same exception, once.** npm, like crates.io, attaches
   a trusted publisher only to a package that exists, and the name `pscat` is
   not registered yet (it was free on 2026-10-20). Do this during the first
   release whose commit has `npm/package.json` (0.6.2), at the same point:
   after `draft` finishes, before you approve `publish`. Publish **the
   tarball that run's `verify` packed**, not one packed from a checkout, so
   the `npm` stage finds the same integrity and skips:

   ```sh
   gh run download <run-id> --repo PSCatLabs/pscat --name release-inputs --dir ri
   tar -xzOf ri/pscat-0.6.2.tgz package/package.json | grep '"version"'   # 0.6.2
   npm login                                  # an npm account with 2FA on
   npm publish ri/pscat-0.6.2.tgz --access public
   ```

   (npm 11.5.1 or later; `--provenance` needs CI, so this one version has
   none.) Then on npmjs.com → `pscat` → Settings → Trusted Publisher → GitHub
   Actions: organization or user `PSCatLabs`, repository `pscat`, workflow
   `release.yml`, environment `release`. Under "Publishing access", choose
   "Require two-factor authentication and disallow tokens", so the trusted
   publisher is the only way in. Now approve the run: the `npm` stage
   reports `pscat@0.6.2 already on npm (same integrity)`, and every later
   release publishes through OIDC with provenance. If the bootstrap is
   skipped, the `npm` stage fails on its first publish (no credential) after
   crates.io has published; bootstrap then, and **Re-run failed jobs**.

   To only reserve the name ahead of time, publish a placeholder version
   (`0.0.0`, then `npm deprecate pscat@0.0.0 "placeholder"`) the same way;
   the trusted publisher can be added right away, and the first real release
   then publishes through it.

## Cutting a release

1. **Prerequisites.** The one-time setup is done. `PINNED_INDEX` pins the
   font index this release ships with. The `release` issue template's
   checklist is open as an issue: "New issue" → "Release".
2. `scripts/release.sh X.Y.Z` (install git-cliff first:
   `cargo install git-cliff` or `brew install git-cliff`). Pick the version
   with the `semver` CI summary in hand. Before 1.0, a minor version may break
   the library API and a patch may not ([`docs/STABILITY.md`](../STABILITY.md)).
3. Polish `CHANGELOG.md` in the PR if needed. When CI is green, the owner
   merges it (`Cargo.toml` is a CODEOWNERS path).
4. Tag the merge commit and push the tag:

   ```sh
   git switch main && git pull
   git tag -a vX.Y.Z -m "pscat X.Y.Z" && git push origin vX.Y.Z
   ```

5. Watch the `release` run. When `draft` is done, review the draft release on
   the Releases page, then approve the `release` environment.
6. Run the post-release checks below.

### Release candidates

A release candidate puts the binaries in testers' hands before the release
is published anywhere permanent (#854). The plugin launcher (#853)
downloads its binary from a pinned GitHub Release, and Actions artifacts
need a GitHub login, so a candidate has to be a public release. It is a
GitHub **pre-release**:

| | Release `vX.Y.Z` | Release candidate `vX.Y.Z-rc.N` |
|---|---|---|
| `Cargo.toml` and manifests | `X.Y.Z` | `X.Y.Z-rc.N` |
| `CHANGELOG.md` | new section | unchanged |
| Gates in `verify` (pin, quality gate, MCP Inspector, licences, crate acceptance, semver) | run | run, the same |
| Five archives, `SHA256SUMS`, `release-manifest.json` | GitHub Release | GitHub **pre-release** |
| Provenance attestations | yes | yes |
| `release` environment approval | yes | yes |
| crates.io, npm, MCP Registry | published | **never** |
| "Latest" on the Releases page | becomes latest | **never** |
| `post-publish-check` | archives and crate | archives only |

**Why the rc version goes in `Cargo.toml`.** It is the standard Rust
practice (cargo-release does the same), and everything in the pipeline
keys on it. `verify` requires the tag to equal `v` + the `Cargo.toml`
version. The archives are named `pscat-<version>-<target>`, and the
bundle check requires `pscat --version` to print that version. A
candidate built from a `0.7.0` `Cargo.toml` would ship binaries that call
themselves 0.7.0, under the same file names as the real 0.7.0 but with
different bytes. Testers' reports could not say which build they ran. The
cost is that `main` carries `X.Y.Z-rc.N` (in `Cargo.toml` and in the
plugin, npm and `server.json` manifests, and the plugin's release pin) until
the release PR for `X.Y.Z`, which sets all of them to `X.Y.Z`. Plugin users who install from `main` see the rc version in that
window. That is accurate, because the plugin's binary comes from the rc
release.

**Why it still needs the approval.** It publishes public binaries,
signed with the job's OIDC token, so it gets the same owner gate as a
release. The attestations need `id-token: write`, which only the
`release` environment's job has.

**What keeps it off crates.io, npm and the MCP Registry.** There are
three layers:

- `release.yml`: the `publish` job does not mint the crates.io token, set
  up npm or install `mcp-publisher` for a candidate. It runs only the
  stages `version-info` names: `draft`, `publish` and `plugin`.
- `scripts/release_stages.py` reports `crates`, `npm` and `registry` as
  `disabled` for any `-rc.N` version, whatever stages it is given.
- The draft is created with `prerelease=true` and `make_latest=false`, and
  publishing sends both again. GitHub never makes a pre-release "latest".
  A release for the tag whose prerelease flag is wrong (made or edited by
  hand) halts the run.

Only `-rc.N` (N ≥ 1) is accepted. `-beta`, `-alpha` and other semver
pre-releases are refused.

#### Cutting a release candidate (owner)

1. On a clean, up-to-date `main`:

   ```sh
   scripts/release.sh 0.7.0-rc.1
   ```

   This opens the PR "release: 0.7.0-rc.1 (release candidate)", which sets
   the version in `Cargo.toml`, `Cargo.lock` and the plugin, npm and MCP
   Registry manifests, and the plugin's release pin `plugin/pscat-release`
   to `v0.7.0-rc.1` (no hand edit). It does not touch `CHANGELOG.md`. git-cliff is not
   needed for a candidate.
2. When CI is green, merge it.
3. Tag the merge commit and push the tag:

   ```sh
   git switch main && git pull
   git tag -a v0.7.0-rc.1 -m "pscat 0.7.0-rc.1" && git push origin v0.7.0-rc.1
   ```

4. Watch the run (Actions → release). The summary's first line says "real
   release candidate, GitHub pre-release only". The `assemble` job's plan
   lists `draft`, `publish` and `plugin` only.
5. When `draft` is done, the Releases page shows a draft marked
   **Pre-release** with five archives, `SHA256SUMS` and
   `release-manifest.json`. Approve the `release` environment.
6. A good result:
   - <https://github.com/PSCatLabs/pscat/releases/tag/v0.7.0-rc.1> is public,
     with the **Pre-release** badge. The notes start "Release candidate for
     0.7.0, for testing".
   - <https://github.com/PSCatLabs/pscat/releases/latest> still redirects to
     the last stable release (v0.6.3).
   - The publish report says `draft` and `publish` published and `plugin`
     skipped. `crates`, `npm` and `registry` did not run.
   - <https://crates.io/crates/pscat> and `npm view pscat version` still
     show the last stable version.
   - `post-publish-check`: the four `install-archive` jobs are green, and
     the two `install-crate` jobs are skipped.
7. The download URLs a tester or the plugin launcher uses are:

   ```
   https://github.com/PSCatLabs/pscat/releases/download/v0.7.0-rc.1/pscat-0.7.0-rc.1-<target>.tar.gz   (.zip on Windows)
   https://github.com/PSCatLabs/pscat/releases/download/v0.7.0-rc.1/SHA256SUMS
   ```

A fix found in testing merges to `main` as usual, then
`scripts/release.sh 0.7.0-rc.2` and the same steps. To release, run
`scripts/release.sh 0.7.0` from `main`, which still says `0.7.0-rc.N`.
That is the normal release procedure above. Its `CHANGELOG.md` section
covers everything since the last stable release, because `cliff.toml`'s
`tag_pattern` ignores rc tags. A candidate's GitHub pre-release stays up
afterwards, like every release (see
[Yanking and deprecating](#yanking-and-deprecating)).

### Release checklist

The same list is in the `release` issue template (`.github/ISSUE_TEMPLATE/release.yml`).

Before tagging:

- [ ] The first `fonts-*` release is published, and `src/fontpacks/pinned.rs`
      pins the index this release ships with.
- [ ] CI is green on the release PR, including `semver` (does the version
      number fit the API change?).
- [ ] The MCP safety tests (`tests/mcp.rs`) and the `mcp` workflow's
      Inspector check pass. `verify` reruns both on the tagged commit.
- [ ] README's install section lists only the channels this release ships.
      Add `cargo install pscat` once the crate is published, and npm or the
      plugin only when they ship.
- [ ] `SECURITY.md`, `THIRD_PARTY_NOTICES.md` and `docs/STABILITY.md` are
      accurate for this release.
- [ ] `CHANGELOG.md` reads well.
- [ ] The owner read the visible changes merged since the last release
      ([`review-log/pending/`](review-log/README.md), #674); each rejected
      entry has a revert PR or a fix ticket. The release PR moves
      `pending/*` to `review-log/vX.Y.Z/` (`git mv`).

After publishing:

- [ ] The GitHub Release is public with five archives, `SHA256SUMS` and
      `release-manifest.json`. crates.io shows the version, and docs.rs built
      it.
- [ ] The `post-publish-check` job (below) is green.
- [ ] **Outsider install smoke test** (below) passes.
- [ ] The release issue is closed with a link to the run's release report.

### Post-publish install check

After `publish`, the `post-publish-check` job runs
`.github/workflows/release-check.yml` with the version just released. It needs
no credentials and no environment: it does what an outsider would.

- **Archives** (Linux x86_64 and ARM64, macOS arm64, Windows x86_64): download
  from the public release URL, check against `SHA256SUMS`, `gh attestation
  verify` the archive, run `pscat --version` and `pscat-mcp --version` (must
  print the release version), render a page and run `pscat handwrite` from an
  empty directory with an empty home. On Linux x86_64 it also runs `pscat
  fonts install` and renders a font-pack face.
- **Crate** (Linux, macOS): wait up to 10 minutes for the version to reach
  the crates.io index, `cargo install pscat --version X.Y.Z --locked`, run
  `--version` and render a page. It also verifies the attestation of the
  `.crate` crates.io serves; a mismatch there is a warning only, because the
  crates stage also accepts a contents-equal crate from the same commit (see
  [Safe resume](#safe-resume)).

A failure means a published release does not install. Nothing can be
unpublished: fix forward with a patch release. For a release candidate it
runs the archive checks only (the crate is not on crates.io). To re-run the check for any
published version, X.Y.Z or X.Y.Z-rc.N: Actions → release-check → Run workflow (empty version =
crates.io's newest; tick `attested` only for releases published with
attestations). A pull request that edits `release-check.yml` runs it against
crates.io's newest version. Versions before 0.6.1 have no `--version`, so the
check runs a program instead.

It does not replace the manual smoke test below: that one also tests the
README.

### Outsider install smoke test

Use a machine, or a container, that has never seen the repo, and follow
**only** the README and the guides it links to.

```sh
# Binary, Linux, in a throwaway container:
docker run --rm -it ubuntu:24.04 bash
apt-get update && apt-get install -y curl ca-certificates
curl -LO https://github.com/PSCatLabs/pscat/releases/download/vX.Y.Z/pscat-X.Y.Z-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/PSCatLabs/pscat/releases/download/vX.Y.Z/SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing
tar xzf pscat-X.Y.Z-x86_64-unknown-linux-musl.tar.gz && export PATH="$PWD/pscat-X.Y.Z-x86_64-unknown-linux-musl:$PATH"
pscat --version && pscat-mcp --version           # must print pscat X.Y.Z, pscat-mcp X.Y.Z
echo '100 100 moveto 300 300 lineto stroke showpage' > t.ps && pscat --png t.png t.ps && ls -l t.png
pscat fonts install && pscat fonts list          # pinned packs, verified
pscat handwrite -o note.png "hello"

# Crate, in a throwaway Rust container:
docker run --rm -it rust:latest bash
cargo install pscat --no-default-features
pscat --version                                  # must print pscat X.Y.Z
echo '100 100 moveto 300 300 lineto stroke showpage' > t.ps && pscat --png t.png t.ps && ls -l t.png
```

On macOS and Windows, download the archive from the Releases page in a
browser, unpack it, and run the same `pscat` commands, starting with `pscat
--version`. On macOS, a binary
downloaded by a browser is quarantined: `xattr -d com.apple.quarantine pscat
pscat-mcp`. Write down anything the README did not tell you, and fix the
README.

### Verifying a release

Every archive and `.crate` released after #635 carries a signed SLSA build
provenance attestation (0.6.0 and earlier have none). With the GitHub CLI:

```sh
gh attestation verify pscat-X.Y.Z-<target>.tar.gz --repo PSCatLabs/pscat \
  --signer-workflow PSCatLabs/pscat/.github/workflows/release.yml
```

It checks that the file's digest was attested by this repository's
`release.yml`, and prints the tag and commit it was built from. `SHA256SUMS`
and `release-manifest.json` record the same digests unsigned.

## Dry runs and tests

- **A pull request** that touches the pipeline (`release.yml`,
  `scripts/release*.{sh,py}`, `package_release.sh`, `cliff.toml`,
  `about.*`, `Cargo.toml`, `Cargo.lock`) runs `verify`, `binaries` and
  `assemble`. That is all five archives, the manifest, the rehearsal and the
  read-only plan. Nothing is published.
- **A manual dry run:** Actions → release → Run workflow, with an optional
  branch, tag or commit as `ref`. It is always a dry run. A real release
  starts only from a pushed tag. To rehearse a **release candidate**, point
  `ref` at a scratch commit made with
  `python3 scripts/release_stages.py bump-manifests X.Y.Z-rc.N` (push that
  as a branch, never a tag). Its rehearsal runs the rc scenarios below.
- **The rehearsal** (`python3 scripts/release_stages.py rehearse --dist DIR
  --crate FILE`) runs these scenarios against fresh fake registries with the
  real artifacts:
  - a plan that changes nothing;
  - the end-to-end publish;
  - a rerun that skips every stage;
  - crates.io succeeding, then a failure publishing the GitHub Release, then
    a rerun that finishes without republishing;
  - a different `.crate` at the version (halts);
  - a different archive in a published release (halts);
  - a draft built from another commit (halts);
  - a moved tag (halts before anything is created).

  For a release candidate it runs these instead:
  - a plan that changes nothing;
  - a GitHub pre-release that never becomes latest;
  - a rerun that skips every stage;
  - `crates`, `npm` and `registry` staying disabled even when asked for, with
    a different `.crate` planted at the version;
  - a failed publish that resumes;
  - a draft not marked pre-release (halts).
- **The post-publish install check** runs on a pull request that edits
  `release-check.yml`, against crates.io's newest version, and on demand
  (Actions → release-check → Run workflow).
- **Unit tests:** `python3 scripts/test_release_stages.py`. It covers the
  stage rules against the fake registry, `release.sh --no-push` in a scratch
  clone (needs git-cliff), and the channel switches. Run
  `python3 scripts/test_check_workflows.py` for the workflow policy. CI's
  `release-policy` job runs both.
- **Locally:** `./scripts/package_release.sh` (needs cargo-about, or pass
  `--no-notices`). Then `python3 scripts/release_stages.py manifest --tag
  vX.Y.Z --commit $(git rev-parse HEAD) --dist dist --crate
  target/package/pscat-X.Y.Z.crate`, and `rehearse` as above.

## Channels

| Channel | Enabled when | Stage |
|---|---|---|
| GitHub Release binaries | always | `draft`, `publish` |
| crates.io | always | `crates` |
| npm (#566) | `npm/package.json` exists | `npm` |
| Claude Code plugin (#569) | a `.claude-plugin/` manifest exists | `plugin`: the manifests must carry the release version, which `release.sh` sets |
| MCP Registry (#569) | `server.json` exists | `registry`, last |

**The plugin stage checks the manifests; it does not change them.** The
issue's stage 6 says "update the plugin manifests". Here `release.sh` sets
their version (and the launcher's `plugin/pscat-release` pin) in the release
PR, so the merged and tagged commit already carries it. Nothing is pushed after the release, and the stage halts if a
manifest disagrees. If #569's marketplace pins a release ref (a `ref:` or
`version` that must point at the published tag), that update belongs in a
step after `publish` that opens a PR. #569 decides.

**How the npm package is built and published (#566).** `verify` runs
`scripts/build_npm.sh --pack release-inputs` after packaging the crate: the
same wasm build as the site (`scripts/build_wasm.sh`), `web/pscat.js` as the
package's `core.js`, the licence files and the same `cargo-about` notices as
the binaries. The script checks that `npm/package.json` carries
`Cargo.toml`'s version, that the tarball holds exactly the expected files,
and that it fits the size budget (4 MiB packed, 9 MiB unpacked). The
tarball travels in the `release-inputs` artifact, so `assemble`'s plan and
rehearsal and `publish`'s `npm` stage all see the same file, and a rerun
publishes the same bytes instead of a rebuild that might differ. The
`npm` workflow (`.github/workflows/npm.yml`) builds and packs it on every
pull request that touches it, runs the API tests, compares wasm pages with
native ones (`tests/wasm.rs`) and installs the tarball into a fresh project.

**How the MCP Registry entry is published (#569).** `server.json` names
`io.github.PSCatLabs/pscat` with one cargo package, `pscat`, run as
`pscat mcp` (a cargo package is launched by its crate name, so the server
answers to the `mcp` subcommand as well as `pscat-mcp`). The registry
proves ownership from a visible `mcp-name: io.github.PSCatLabs/pscat` line
in the README crates.io serves for the version (crates.io strips HTML
comments). README.md carries it from 0.6.3 on; earlier versions cannot be
registered. The `registry` stage:

1. checks that `server.json` carries the release version (`release.sh`
   bumps it) and that the `.crate`'s README has the token, offline, so the
   plan in `assemble` catches a missing one before anything is published;
2. asks the registry for the version: present and the same skips, present
   and different halts;
3. waits (a few minutes at most) for crates.io to render the README of the
   version the `crates` stage just published, and checks the token there;
4. runs `mcp-publisher login github-oidc`, then `mcp-publisher publish
   server.json`, and reads the entry back.

It runs last, after `publish` and `plugin`, so a registry outage or a
failed login leaves everything else released; **Re-run failed jobs** then
retries only this stage. `mcp-publisher validate` is not used: it posts to
the registry, so `test_release_stages.py` checks the same structure
offline instead (the current schema URL, the description's 100-character
limit, the package and the README token).

A new channel is a new stage in `scripts/release_stages.py` (the same
absent/same/different rule, with a fake backend and tests) and a step in
the `publish` job. No other stage changes. `release.sh` already bumps an
`npm/package.json`, any `.claude-plugin/*.json`, a `server.json` (MCP
Registry) and the plugin's `plugin/pscat-release` pin when they exist.

## Hotfixes

Releases are cut from `main` only. `verify` refuses a tag that is not on
`main`. A hotfix is an ordinary `fix/` PR into `main`, followed by a patch
release (`scripts/release.sh X.Y.Z+1`). If `main` holds work that must not
ship yet, finish or revert it first. Supporting maintenance branches would
mean relaxing `verify`'s ancestry check to `release/*` branches, so do that
deliberately and update this section if it is ever needed.

For a security fix, follow [`SECURITY.md`](../../SECURITY.md): a private
advisory first, the patch release, then publish the advisory (and a RustSec
advisory for the crate).

## Yanking and deprecating

Nothing published is deleted or replaced.

- **crates.io:** `cargo yank --version X.Y.Z pscat` stops new lockfiles from
  picking the version, and `cargo yank --undo` reverses it. Yank only a
  version that is broken or unsafe, and say why in the next release's notes.
- **GitHub Release:** edit the release notes to start with "**Deprecated:**
  use X.Y.Z+1 (reason)". Do not delete the release or its assets, because
  `SHA256SUMS` and downstream pins refer to them.
- **npm:** `npm deprecate pscat@X.Y.Z "use X.Y.Z+1: reason"`. npm allows
  an unpublish only within 72 hours and never reuses the version; deprecate
  instead.
- **Font packs:** never deleted. A pscat release may pin them. Fix forward
  with a new `fonts-*` tag.

## After 0.6.0

`semver` in CI stays informational on pull requests: it compares with the PR's
base commit, because versions are bumped only at release time. From the second
release on, `verify` runs `cargo semver-checks check-release` against the
crates.io baseline, and it blocks.
