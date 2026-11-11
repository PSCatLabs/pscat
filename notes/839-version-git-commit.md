## `--version` names the commit of a git build (issue #839, 2026-10-26)

`cargo install --git ... --rev X` printed `pscat 0.6.3`, the same as the
crates.io release, so a tester's report could not say which build it was.
build.rs now exports `PSCAT_VERSION` (`env!("PSCAT_VERSION")`), used by
`pscat --version`, the REPL and window banners, `pscat-mcp --version` and the
MCP `serverInfo.version`:

- Release, crates.io, tarball, vendored, no `git`, no commits: `0.6.3`.
- Git checkout that is not on a `v[0-9]*` tag: `0.6.3+56006db`, and
  `0.6.3+56006db-dirty` when a build input (`src`, `lib`, `fonts`, build.rs,
  Cargo.toml/lock) differs from HEAD; docs and untracked files do not count.
  On a tag with edits: `0.6.3+dirty`.

Decisions and traps:

- **`.git` must sit beside Cargo.toml.** `git` otherwise walks up and adopts
  an unrelated parent repository (`~/.cargo/registry` under a dotfiles repo,
  a vendored copy in a monorepo). Checked: a `cargo package` tarball unpacked
  inside this repo prints the plain version.
- **`cargo install --git`** checks out a real clone (with `.git`), so it gets
  the commit. Its stray `.cargo-ok` file is untracked, which is why the dirty
  test ignores untracked files (`status --untracked-files=no`).
- **Releases set `PSCAT_RELEASE=1`** in `scripts/package_release.sh`.
  `release.yml` checks out a commit SHA with no tags, so tag detection alone
  would stamp `+sha` on the archive and fail the `--version` equals Cargo
  version check. The tag test is for `cargo install --git --tag`.
- **`capabilities.rs`'s `pscat_version` still reports Cargo's version.** It is
  a drift signal documented as moving only per release.
- Rebuild cost: a commit rebuilds the crate (the string is baked in), as any
  embedded version would. build.rs also watches `src`, `fonts`, Cargo.toml/lock, so a
  source edit re-reads the dirty flag; measured on a bin-only edit, release 2.9 s to
  7.4 s, debug 0.4 s to 1.1 s. Edits that touch the library rebuild it anyway.
- The plugin launcher (#861) accepts `0.7.0+abc1234-dirty`: it reads
  `[0-9][0-9.]*[^ ]*` and strips from the first `-` or `+` before comparing.
- The commit is tracked via `HEAD`, `logs/HEAD`, `refs/heads`, `packed-refs`
  and `refs/tags` (only paths that exist: cargo reruns on a missing one every
  build). `logs/HEAD` matters after `git gc` packs the branch ref: the next
  commit then changes neither HEAD nor packed-refs. Checked with
  `git pack-refs --all`.
