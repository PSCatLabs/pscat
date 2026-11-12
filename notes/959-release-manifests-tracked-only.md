# release_stages: manifest discovery is tracked files only (#959)

`plugin_manifests()` (so `bump-manifests`, so `release.sh`, and the release
`plugin` stage) globbed `**/.claude-plugin/*.json` over the working directory.
Running `release.sh 0.7.0-rc.2` rewrote `codex/functional_ps/plugin/.claude-plugin/plugin.json`,
a separate untracked repository inside the checkout. The release commit was
right (untracked files are not staged) but the script edited someone else's tree.

Now `tracked_files(root)` reads `git ls-files -z` and `plugin_manifests` keeps a
glob hit, the Codex marketplace and each Codex plugin path only if its relative
path is tracked. Rules:

- `root` has a `.git` (directory, or file in a worktree): only tracked paths.
  If `git ls-files` fails there it is a `SystemExit`, never a silent fall back
  to the glob (that would bring the bug back unnoticed).
- `root` has no `.git` (a copied tree, the test fixtures): every file on disk,
  as before.
- Staged-but-uncommitted files count (the index is what `ls-files` reads).

Tests (scripts/test_release_stages.py): a throwaway git repo with an untracked
nested repo and an untracked scratch dir is left byte-identical by
`bump_manifests_json_only`; a broken `.git` is an error. The existing fixture
tests cover the no-`.git` path.
