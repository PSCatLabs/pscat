## pscat.global.js as a GitHub release asset, and its TypeScript global (issue #1108, 2026-11-20)

- `release_stages.py manifest --npm-from DIR` now also unpacks
  `package/pscat.global.js` from the npm tarball into `dist/` and lists it
  after the tarball. So it is in `SHA256SUMS` and `release-manifest.json`,
  `draft` uploads it, and resume/mismatch checks treat it like any asset. Same
  bytes as npm, never a rebuild. A tarball without it stops `assemble`.
  `release.yml` only gains `dist/pscat.global.js` in the attestation subjects.
- `npm/global.d.ts` declares `var pscat` (the three members the script
  defines). It is opt-in (`/// <reference types="pscat/global" />`), so
  `import 'pscat'` does not add a global. Exports: `./global` and the types
  condition on `./pscat.global.js`. `test_npm_tarball.sh` type-checks a consumer
  that uses it (a tarball must have it; a registry release before the file
  existed is skipped); `global.test.mjs` pins the member list, `files` and
  `exports`.
- `release-check.yml` gained `install-global`: from 0.7.1 on (candidates
  included) it downloads the asset from the public URL, checks `SHA256SUMS`,
  loads it as a classic script.

Measured, with headless Chrome against the real rc.5 release URLs: a
`<script src>` of a release asset executes although GitHub serves it as
`application/octet-stream` (so no per-type upload content type is needed), and
a cross-origin `fetch` of one is blocked by CORS (no `Access-Control-Allow-Origin`
on the github.com redirect or the asset host). So the pinned URL works for the
script, but the page must host `pscat.wasm` itself and pass `{ wasm }`; the
docs say so. Not done: a `pscat.wasm` asset of its own (it is in the `.tgz`).
