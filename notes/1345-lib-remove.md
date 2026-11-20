## `pscat lib remove NAME [--force]` (issue #1345, 2026-11-20)

The "Deferred" item of `notes/1056-currentfilename-and-lib-install.md`: a way to
uninstall what `pscat lib install` put in `lib/NAME/` under `Roots::lib_install_dir`.

**Rules.** NAME is one plain component (`[A-Za-z0-9_-]{1,64}`, the install rule), so
`..`, a separator or a dot name cannot reach outside `lib/`. The command looks at
`lib/NAME` with `symlink_metadata`:

- missing: "not installed" (nothing is created, not even `lib/`);
- a symbolic link or a plain file: refused, with or without `--force` (removing the link
  would be safe, but a link under `lib/` was not made by `install`, and the owner of the
  file should remove it);
- a directory holding a `.pscat-lib` that is a regular file (not a link): removed;
- any other directory: refused, naming `--force`; with `--force` it is removed, since that
  is what the flag is for (a hand-made package, or one whose marker was lost).

It prints `removed lib/NAME: N files from <absolute path>`. N counts every non-directory
entry, hidden ones and links included, but not the marker; for an installed package that is
the number `lib list` shows. The absolute path is the receipt: `--force` deletes whatever
the install directory holds under that name, and `$PSCAT_HOME` can point anywhere.

**Interruption.** The whole directory goes in one `fs::remove_dir_all(lib/NAME)`, so a
program that loads `lib/NAME/x.ps` while it runs may find some files missing, and a removal
that is killed half way can leave a partial directory (possibly without its marker). Run
`pscat lib remove NAME --force` to finish it. It is not renamed aside first, to keep this
clear of the scratch-directory handling of #1344.

**Why `remove_dir_all` is safe against a swap.** It is called on `lib/NAME` itself. It does
not follow a symbolic link, at the top or below, and since the CVE-2022-21658 fix (Rust
1.58.1) it walks with directory handles, so a directory swapped for a link during the walk
is unlinked, not entered. The `symlink_metadata` check and the removal are two calls, but a
link swapped in between is unlinked rather than followed. (The walk is deliberately not
written by hand: a `read_dir` of the path would follow such a link.) `count_all`, which
only reads, may follow a swap; that affects the number printed, nothing else.

**Known residual.** `install` copies each source file's permissions, so a read-only file
inside a package may make removal fail on Windows (a read-only attribute blocks delete
there); the error names the path. A re-install's cleanup of the old copy has the same limit.

Does not touch a stale `.NAME.new-<pid>` / `.NAME.old-<pid>` scratch directory: `remove`
looks at `lib/NAME` only. Sweeping those is #1344.
