# notes/ — per-issue notes fragments

New notes go here, one file per issue: `notes/<issue>-<slug>.md`
(for example `notes/273-per-issue-notes-fragments.md`). Two concurrent PRs
then never touch the same file, so they cannot conflict on notes (#273).
Before this, every PR prepended a section to `NOTES.md`, and any two open
PRs conflicted there.

- **`NOTES.md` is frozen history.** Everything written up to 2026-10-13
  stays in it, in place. Do not add new sections to it, and do not fold
  fragments back into it: that would bring the conflict back.
- **`notes/` holds the newer entries.** To list them newest first:
  `awk 'FNR==1' notes/[0-9]*.md | sed 's/.*, \([0-9-]*\))$/\1 &/' | sort -r`
  (every fragment's first line carries its date).
- **Format.** The first line is a heading in `NOTES.md`'s style,
  `## <title> (issue #N, YYYY-MM-DD)`, then the usual body: what was built,
  tradeoffs, what is deferred. Issue numbers are not chronological, so the
  date in the heading is what orders fragments.
- **One file per issue.** A follow-up PR for the same issue appends to
  that issue's fragment; a different issue gets its own file. If a file of
  that name already exists on `main`, you are extending it, not replacing it.
- If you do touch `NOTES.md` (fixing an old entry), keep the edit small and
  away from the top of the file.
