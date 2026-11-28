## `package_release.sh` removes its scratch directories on every exit (issue #1486, 2026-11-28)

The script makes three `mktemp -d` directories while it checks the bundle
(`EMPTY_HOME`, `TOOLS`, `SYMLINK_DIR`). Each was removed by an inline `rm -rf`
on the success path only. At most one of them is live at a time (each is
removed inline before the next is made), so a `FAIL:` exit leaked the one that
was live at that moment, and a `FAIL` after its inline `rm` left none.

Now the variables are declared empty before the first `mktemp` and an `EXIT`
trap (`cleanup_scratch`) removes whichever are set, on every way out. The inline
`rm -rf`s stay, so the success path still tidies up early and its output is
unchanged.

Decisions and traps:

- **The cleanup function never changes the exit status.** Under `set -e` a
  trailing `[[ -n "$d" ]] && rm ...` that ends false would turn an `exit 0` into
  exit 1, so the test is an `if`, the function ends in `return 0`, and the
  removal is `rm -rf "$d" || true`: a failing `rm` (a stub exiting 9 turned exit
  0 and exit 7 into 9 before that) neither replaces the script's status nor
  skips the remaining directories.
- **Subshells do not inherit the trap**, which is right, and the directories are
  still seen by it: each `mktemp -d` runs inside a `$(...)` command
  substitution, but the parent shell assigns the returned path to the variable,
  so the trap (which runs in the parent) knows it. The `( ... )` subshells
  (`cargo about`, the zip step) create none of them.
- **Nothing handed to the caller is removed**: the archive lives under
  `$OUT_BASE`, not in these three directories.
- Failures before line `EMPTY_HOME=$(mktemp -d)` (a bad option, a missing
  cargo-about or notices file) never leaked; only the checks after it did.
- A hard kill (`SIGKILL`) cannot run any trap; `INT` and `TERM` do run it in bash.
