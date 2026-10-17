## Cross-platform CI, MSRV, packaging checks, cargo-deny (issue #601, 2026-10-16)

Part of release epic #562 (split from #567). Catches platform and packaging
problems in PR CI before the publishing pipeline (#602) exists.

**Built.** Everything is additive in `.github/workflows/ci.yml`, below the
unchanged `test` job.
- `cross-platform (ubuntu-latest)` and `cross-platform (windows-latest)`:
  selftest plus `cargo test --profile ci`. A separate job, not a matrix on
  `test`: the required check is named `test`, and a matrix would rename it to
  `test (macos-latest)` and leave the required check pending forever. The
  Windows job sets `core.autocrlf=false` before checkout (the repo is LF).
- `msrv`: reads `rust-version` from `Cargo.toml`, `cargo check --all-targets
  --locked` on that toolchain.
- `packaging`: on PRs touching `Cargo.toml`/`Cargo.lock`/`build.rs`/`deny.toml`/
  `lib/`/`fonts/`/licence files/`licenses/` (always on pushes to main),
  `scripts/check_licences.py package` then `cargo publish --dry-run --locked`.
- `licences`: `scripts/check_licences.py all` (font-catalog licence files,
  `licenses/assets.json` coverage) and the checker's unit tests.
- `deny`: `cargo-deny` with `deny.toml` (licence allowlist, advisories, bans,
  sources).

**`scripts/check_licences.py`.**
- `fonts`: every `fonts/catalog/*/` has a licence-named file (OFL, LICENSE,
  COPYING, GUST-FONT-LICENSE, ...) beside the font.
- `assets`: `assets.json` ends in a `**` catch-all, so "every tracked file
  matches" is vacuous. The check is instead that no font (anywhere) and no
  image/PDF outside the project's own render trees (`docs/`, `notes/`, `lib/`,
  `site/`, `tests/`, ...; `refs/` directories are never exempt) falls to the
  catch-all, that every non-catch-all entry matches a tracked file (a stale
  entry means a rename dropped a licence), and that named licence files exist.
- `package`: parses `cargo package --list`; requires the licence files; every
  listed file must match an `assets.json` entry that ships in `crate`; catalog,
  gallery, examples, site, docs, art and tests are forbidden outright.

**Advisories.** `cargo deny check` found three real ones, ignored in `deny.toml`
with reasons: `quick-xml` 0.39 (RUSTSEC-2026-0194/0195; build-time proc macro
via wayland-scanner, not upgradeable, #611) and `ttf-parser` unmaintained
(RUSTSEC-2026-0192, #610). Retire each entry when its issue lands.

**Windows fixes.** The first Windows run found real problems, all fixed:
- `scripts/selftest.sh` looked for `target/<profile>/pscat`; under Git Bash it
  needs `.exe`, and `cargo metadata` JSON-escapes backslashes.
- `pscat-mcp` looked for a sibling binary named `pscat`, so on Windows
  `render_postscript` and `eval_postscript` always failed (user-facing bug); it
  now uses `EXE_SUFFIX`.
- Tests that put a temp path inside a PostScript string literal lost it on
  Windows (`C:\Users\x` reads as `C:Usersx`): `etching` and `selftest` now use
  `common::ps_path`. `selftest`'s manifest check compared `lib\artkit.ps`.
- `src/paths.rs`'s `/abs/x.ps` test is `C:/abs/x.ps` on Windows.
- `scripts/clean_install_test.sh`: `pscat.exe`, and `cargo package --list`
  prints `lib\x.ps` on Windows. `clean-install` gained a `windows-latest` leg.

**Skipped on Windows (#615).** Three tests drive `handwrite.sh`/`typewrite.sh`
through bash and are `#[cfg_attr(windows, ignore)]`; #600 ports those scripts,
and #615 re-enables them. One issue, not three, because #600 is the one fix.

**Timing.** Full suite, no subset: macOS `test` ~6 min, ubuntu ~8 min, Windows
~11 min (selftest ~1 min of it). `--no-fail-fast` on the new job so one bad
test binary on a new platform does not hide the rest.

**Demonstration.** Throwaway PR #613 (closed) added `sequoia-openpgp` (LGPL)
and an unlicensed `fonts/catalog/DemoNoLicence/`: `deny` and `licences`
failed (run 38984063204). `deny.toml` is an allowlist, so any GPL crate fails
the same way.

**Open.** `deny` pulls the live advisory database, so it can turn red on an
unrelated PR the day an advisory lands; the owner may prefer to keep it
non-required.
