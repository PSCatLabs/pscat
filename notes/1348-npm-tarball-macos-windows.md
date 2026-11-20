## npm.yml installs the tarball on macOS and Windows (issue #1348, 2026-11-20)

`npm.yml` built the package and ran `scripts/test_npm_tarball.sh` on Linux
only. A package that breaks only on macOS or Windows (a path separator, the
exports map, `pscat/node.js` resolution) showed first in the post-publish
`install-npm` job of `release-check.yml` (#668), when it could no longer be
pulled.

- The `package` job (Linux, unchanged) now also uploads `pack/pscat-*.tgz`
  as the `npm-tarball` artifact (1 day retention).
- New job `install-tarball` (macOS and Windows, `fail-fast: false`) needs
  `package`, downloads that artifact and runs
  `scripts/test_npm_tarball.sh pack/pscat-*.tgz`, the same full check
  (install, render, SVG/PDF, `pscat/node.js`, `pscat.global.js`, TypeScript).
  It checks out only that script and `.gitattributes` (LF on Windows), as the
  `install-npm` job does.
- The tarball is built once, on Linux, because that is how `release.yml`
  builds the one it publishes; installing that file elsewhere tests what users
  get. Building the wasm and the notices on Windows and macOS would test the
  contributor toolchain, not the package.

Triggers, permissions (`contents: read`) and secrets are unchanged.
