# #867: bump-manifests sets plugin/pscat-release

Follow-up to #853 and #854. `plugin/pscat-release` holds the release tag the
plugin launcher downloads. `plugin_check.py` requires it to equal
`v<crate version>`, or to be a pre-release of a version not older than the
crate. `bump-manifests` did not touch it, so every release PR, rc included,
needed a hand edit (or failed the `plugin` workflow).

**What.** `release_stages.py set_release_pin(root, version)`, called from
`bump_manifests`:

- rewrites the one non-comment line to `v<version>` (rc versions give
  `v0.7.0-rc.1`), keeping the comment lines and the trailing newline;
- writes only when the content changes, and returns the path
  `plugin/pscat-release` in `bump-manifests`' output. `release.sh` stages
  exactly that list and commits without `-a`, so the path has to be listed;
- leaves a tree without the file alone (`test_cargo_version_bump`);
- raises `SystemExit` on zero or several tag lines, as `plugin_check` would
  reject them, rather than guess.

**Design calls.**

- The pin is set with the other versions, not as a separate step, because the
  release PR is the only place the crate version changes.
- `stage_plugin` (the post-publish check) is unchanged: `plugin_check.py` is
  the pin's gate, and it runs on every PR.
- The 0.7.0 release PR after an rc now sets the pin back to `v0.7.0`. Before,
  `plugin_check` did not catch an rc pin of the same version as `Cargo.toml`
  (`v0.7.0-rc.1` with Cargo at `0.7.0`), so that step was manual and
  unchecked.

**Tests** (`scripts/test_release_stages.py`, class `PluginPin`):

- the helper (comments kept, idempotent, absent file, malformed file);
- `bump_manifests` lists the pin;
- a copy of the real tree bumped to `99.0.0-rc.1` and then `99.0.0` passes
  `plugin_check.py` each time (and fails with the old pin);
- `ReleaseScript` (needs git-cliff, installed in CI): the commit made by
  `release.sh` contains the pin, set to `v99.0.0-rc.1` for the rc.
- Mutation: disabling the call fails four of these.
