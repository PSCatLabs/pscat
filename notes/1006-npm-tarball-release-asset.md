## The npm tarball as a GitHub release asset, release candidates included (issue #1006, 2026-11-04)

Release candidates never go to crates.io, npm or the MCP Registry (#854),
so a browser or npm consumer (PSCatLabs/print-shop) had no released artifact
to pin a candidate to. `verify` already packed the npm tarball; now it ships.

- `release_stages.py manifest --npm-from DIR`: when `npm/package.json`
  exists, the tarball `verify` packed into DIR (named as `npm pack` names
  it, from package.json's name, which is how the `npm` stage finds it) is
  copied into `dist/` and listed in `release-manifest.json` after the
  archives, so it is in `SHA256SUMS` and the `draft` stage uploads it. A
  missing tarball, or one holding another package or version, stops
  `assemble`, so a release cannot quietly ship without it. With no
  `npm/package.json` the flag does nothing.
- The asset rides the existing resume rules: a draft or published release
  is judged against its manifest's assets, so the tarball is checked,
  completed or halted on exactly like an archive.
- The `npm` stage also halts unless the tarball it is about to publish has
  the SHA-256 the manifest lists: npm and the GitHub Release carry the same
  bytes.
- The publish job attests `dist/*.tgz` with the archives and the `.crate`.
- The release notes name the tarball and give the
  `npm install https://github.com/PSCatLabs/pscat/releases/download/vX/pscat-X.tgz`
  line; for a candidate they say it is not on npm.
- The rc invariant is unchanged: the `npm` stage is still `disabled` for an
  `-rc.N` version, and the publish job still sets up no npm for one. The rc
  tests now also assert the pre-release carries the `.tgz` and npm saw
  nothing.

Docs: `docs/dev/RELEASING.md` (pipeline table, rc table and steps, "Testing a
candidate's npm package", verifying), `docs/guides/browser.md`, the release
issue template.

Deferred: an automated install of the release's `.tgz` in
`release-check.yml` (the post-publish check). For now the owner checks it by
hand after rc.4 (`scripts/test_npm_tarball.sh`).
