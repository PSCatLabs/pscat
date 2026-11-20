# #1344: `pscat lib install` sweeps the stage a killed install left

`pscat lib install` builds the new copy in `.NAME.new-<pid>`, then swaps it in (#1056). A kill in between left the
dot-directory for good.

- The stage gets a `.pscat-lib-stage` marker first; it is removed just before the swap, so it never reaches the package.
- An install starts (after `lib/` is created, before its own stage) by deleting each stage in `lib/` that is stale AND ours.
  Stale: on Unix, its pid is not running (`kill(pid, 0)`; a pid that is not a valid process id counts as not running).
  Age plays no part there: a live pid may be a suspended or stalled install, whose stage must survive. A stage whose pid
  is running (an install, or a reused pid) is left, and `list` says it can be removed by hand; a reused pid costs one
  harmless directory, the safe failure. Where a pid cannot be probed (not Unix), a stage is stale once it is over a day
  old. Ours: it holds `.pscat-lib-stage` or `.pscat-lib` as a real file (checked with `symlink_metadata`: a link
  standing in for a marker would let a hand-made directory be deleted), or is empty (killed between `mkdir` and the
  marker write). A pid equal to this process's own is not special: the stage does not exist yet,
  so a stage with this pid is a dead process's, and `make_scratch` would refuse it.
- Never touched: a link, a plain file, a hand-made directory with content and no marker, the stage of a running install.
- `pscat lib list` prints each stale stage, and says either that the next install removes it or that pscat did not make
  it and it must go by hand. List deletes nothing.
- No pid probe off Unix: there only the one-day age applies.
- `.NAME.old-<pid>` holders are not swept: after a failed second rename one holds the only copy of the old package.

## Residuals

- Check-then-delete race: someone who can rename `lib/` aside and put a symlink in its place between the marker check and
  `remove_dir_all` could point the delete elsewhere (std has no handle-bound recursive delete). That needs write access to
  the user's own library directory, the same trust boundary as the library itself. Known and accepted, not fixed.
