## Downloadable font packs, `pscat fonts`, the fonts-* release train (issue #599, 2026-10-16)

The font catalog now reaches installed pscat builds as three downloadable
packs, built from `fonts/catalog/` and released on their own
`fonts-YYYY.MM` train. Only the twelve Liberation faces stay compiled
in. Part of the release epic #562, split from #564; the design was
agreed in #564's thread.

**What shipped.**
- `fonts/packs.json` assigns each family to a pack: `extras` (3.0 MB:
  the OFL/Apache display and text faces, Courier Prime, Special Elite,
  URW Symbol/Dingbats), `texgyre` (1.1 MB, GUST) and `cjk-thai` (10.0 MB).
  Sizes are zstd -19 of the current catalog. The file is compiled in, and
  a test keeps it equal to `fonts/catalog/`.
- `scripts/font_packs.py` builds deterministic `.tar.zst` packs. Each
  carries the font files, the licence files #563's `licenses/assets.json`
  lists for the family (URW gets only LICENSE.md, LICENSE.OFL and
  PROVENANCE.md), and a `manifest.json` with each file's SPDX id,
  copyright and SHA-256. The script also writes `font-index.json` and
  `SHA256SUMS`, and `pin` writes `src/fontpacks/pinned.rs`.
- `src/fontpacks/` holds the pack table, the installed-pack discovery,
  a strict small ustar reader (regular files only, no traversal) over
  pure-Rust `ruzstd`, the installer, and `pscat fonts install|list|remove`.
  `main.rs` only dispatches to it.
- Font lookup (`font.rs`) now scans every `fonts/catalog/` override and
  then the installed packs, where it used to read one catalog dir. A
  pack face that is on no disk is `invalidfont`, and `error_report` adds
  a second line naming the pack. That includes /Symbol and
  /ZapfDingbats. This closes the gap #598 left, where a release binary
  outside the checkout silently substituted Helvetica. Non-pack names
  still substitute.
- `--fonts` ends with `#` status lines, and `--capabilities` has a
  `font_packs` object.
- `.github/workflows/release-fonts.yml` runs for a `fonts-*` tag, on
  workflow_dispatch, and on PRs touching the packs. It builds, pins a
  throwaway pscat to the fresh index, runs `scripts/font_pack_smoke.sh`,
  uploads an artifact, and publishes only for a tag and only with
  `vars.FONTS_RELEASE_ENABLED == 'true'` (#602).
- `package_release.sh` no longer bundles the catalog.
  `clean_install_test.sh` asserts the missing-pack error.

**Decisions.**
- *curl, not an HTTP crate.* rustls needs a root store (webpki-roots is
  CDLA-Permissive-2.0, outside #601's allowlist) or a platform verifier
  with its own per-OS tree. curl is on every supported OS, uses the OS
  trust store, and integrity comes from the SHA-256 chain anyway. The new
  crates are `ruzstd` (MIT) and `sha2` and its RustCrypto deps (MIT OR
  Apache-2.0), all native-only.
- *Pinned means the pinned bytes.* A pack is reported pinned when the
  index it came through matches the binary's pinned tag and SHA-256,
  whichever command installed it. So `--version <pinned tag>` reports
  pinned. A lone `--from pack.tar.zst` has no index and is always
  user-selected.
- *Overrides beat packs.* Every plain `fonts/catalog/` directory
  (PSCAT_HOME, user data, exe dir, checkout in debug builds, cwd) is an
  explicit choice and wins over installed packs. So a checkout's golden
  renders never depend on what packs a developer has installed.
- *All or nothing.* Every requested pack is downloaded and verified into
  `.staging-*` directories before any is renamed into place. A drop
  guard removes staging on any error.
- *A bare `pscat fonts install` with no pin is an error*, never a silent
  `latest`.

**Verified.** Unit tests cover install from a URL with a fake fetcher,
pinned and user-selected status, pin mismatch, checksum mismatch with
nothing partial left behind, latest, local index and pack files, and
the archive edge cases. Integration tests (`tests/font_packs.rs`) cover
an override beating a pack, `--fonts`/`--capabilities` status, a
tampered pack, CLI usage, and a script-built pack through the Rust
reader. The smoke script, run locally both unpinned and pinned against
a real build, renders five catalog-font examples (six pages) byte for
byte identically from the packs and from `fonts/catalog/`.

**Deferred.** #616: refuse an interpreter release whose index is
unpinned. #617: font packs in the wasm build. The live GitHub URL path
first runs for real with the first published `fonts-*` release (#602);
until then it is covered only by the fake-fetcher tests.
