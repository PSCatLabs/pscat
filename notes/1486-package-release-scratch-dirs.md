## `package_release.sh` removes its scratch directories on every exit (issue #1486, 2026-11-28)

The script makes three `mktemp -d` directories while it checks the bundle
(`EMPTY_HOME`, `TOOLS`, `SYMLINK_DIR`). Each was removed by an inline `rm -rf`
on the success path only, so every `FAIL:` exit after the first `mktemp` left
one to three of them behind in `$TMPDIR`.

Now the variables are declared empty before the first `mktemp` and an `EXIT`
trap (`cleanup_scratch`) removes whichever are set, on every way out. The inline
`rm -rf`s stay, so the success path still tidies up early and its output is
unchanged.

Decisions and traps:

- **The cleanup function always returns 0.** Under `set -e` a trailing
  `[[ -n "$d" ]] && rm ...` that ends false would turn an `exit 0` into exit 1.
- **Subshells do not inherit the trap**, which is right: the `( ... )` and
  `$( ... )` forms in the script never create these directories.
- **Nothing handed to the caller is removed**: the archive lives under
  `$OUT_BASE`, not in these three directories.
- Failures before line `EMPTY_HOME=$(mktemp -d)` (a bad option, a missing
  cargo-about or notices file) never leaked; only the checks after it did.
- A hard kill (`SIGKILL`) cannot run any trap; `INT` and `TERM` do run it in bash.
