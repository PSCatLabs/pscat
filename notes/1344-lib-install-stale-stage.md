# #1344: `pscat lib install` sweeps the stage a killed install left

`pscat lib install` builds the new copy in `.NAME.new-<pid>`, then swaps it in (#1056). A kill in between left the
dot-directory for good.

- The stage gets a `.pscat-lib-stage` marker first; it is removed just before the swap, so it never reaches the package.
- An install starts (after `lib/` is created, before its own stage) by deleting each stage in `lib/` that is stale AND ours.
  Stale: its pid is not running (`kill(pid, 0)`; a pid that is not a valid process id counts as not running), or the
  directory is over a day old (pids are reused). Ours: it holds `.pscat-lib-stage` or `.pscat-lib`, or is empty (killed
  between `mkdir` and the marker write). A pid equal to this process's own is not special: the stage does not exist yet,
  so a stage with this pid is a dead process's, and `make_scratch` would refuse it.
- Never touched: a link, a plain file, a hand-made directory with content and no marker, the stage of a running install.
- `pscat lib list` prints each stale stage, and says either that the next install removes it or that pscat did not make
  it and it must go by hand. List deletes nothing.
- No pid probe off Unix: there only the one-day age applies.
- `.NAME.old-<pid>` holders are not swept: after a failed second rename one holds the only copy of the old package.
