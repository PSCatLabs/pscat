## release-check installs the npm package after publish (issue #668, 2026-11-19)

`release-check.yml` checked the archives and the crate; the npm package
(#566) was published and never installed from the registry afterwards.

- New job `install-npm` (Linux, macOS, Windows). It waits up to 10 minutes
  for `pscat@X.Y.Z` on the registry, then runs
  `scripts/test_npm_tarball.sh pscat@X.Y.Z` (`--attested` when the check is
  run with `attested`). Only that script is checked out (sparse), so the
  package under test is the registry's, while the script is the one at the
  workflow's ref: the release tag when `release.yml` calls it.
- `test_npm_tarball.sh` now takes a registry spec as well as a `.tgz`
  (a file is a tarball, anything else a spec; a bare `X.Y.Z` means
  `pscat@X.Y.Z`). For a spec it asserts the installed version, runs
  `npm audit signatures` (must pass and report a verified registry
  signature) and, with `--attested`, requires the verified provenance
  attestation: `npm audit signatures` exits 0 for a package with no
  attestation, so the script reads its report.
- A published release before 0.7.0 (0.6.3 is what the pull-request run
  of the workflow installs, crates.io's newest) has no SVG/PDF output, no
  `pscat/node.js` export and no `pscat.global.js`, and its `index.d.ts` has no
  SVG/PDF, so for those the script skips that part. A tarball is always
  checked in full.
- The job is skipped for a release candidate (not on npm) and for versions
  before 0.6.2 (no package).
- `.gitattributes` pins the script to LF so a Windows checkout can run it.

Not done: the 0.6.2 registry package has no provenance (published by hand),
so `--attested` fails for it by design (provenance starts at 0.6.3). The
attestation's signer identity is not pinned to `release.yml` as it is for the
archives: `npm audit signatures` only says it verifies. The script retries up
to 3 times in the workflow, 30 s apart, for registry lag after `publish`.
