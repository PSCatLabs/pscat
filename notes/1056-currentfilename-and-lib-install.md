## `currentfilename` and `pscat lib install` (issue #1056, 2026-11-19)

Parts 2 and 3 of #1056, owner decision 2A and 3B. Part 1 (documenting the `lib/`
search order) shipped in #1078.

**2A: `currentfilename`.** A pscat extension (`- currentfilename string`). It
returns the name the running file was opened with, as given: `lib/sheet.ps`,
the path as written to `run`, or the file argument as written on the command
line. `undefined` outside a named file (`-e`, stdin `-`, the REPL, an MCP tool
call's program). Run files already carry their name for the error report's
`Line:` (`Lexer::run_name`, #1043); the operator reuses that. The main program
has no `run_name` (its lines stay a bare number), so the CLI names it through a
separate `Interp::set_main_file_name`, set in `Options::new_interp`. The walk
over the execution stack takes the first scanner that is a named run file or the
main program, skipping eexec streams, executable strings and file objects, so
`{ currentfilename } exec` inside a library is the library. It reports the file
running at the moment of the call: a procedure defined in a library and called
from the main program answers with the main program's name. Libraries read it
at their own top level (`/dir currentfilename dirname def`); `dirname` is a few
lines of PostScript, in getting-started.md. Not in Ghostscript, so it is in
STABILITY.md as an extension and the capabilities catalog.

**3B: `pscat lib install DIR [--name NAME]`, `pscat lib list`.** Copies DIR to
`lib/NAME/` under `Roots::lib_install_dir`: `$PSCAT_HOME` if set, else the
per-user data directory (the choice `pscat fonts install` makes, and what makes
the tests hermetic). No change to the search order or the read roots, so
nothing changes for pscat-mcp: the directory is already step 2 or 4 of the
search and a read root of the server.

The one risk is shadowing. Step 2 and step 4 rank before the compiled-in copy
(step 6), so a file at `<install>/lib/artkit.ps` would replace the built-in
library everywhere, which is the property pscat-mcp#87 relies on never
happening. So `install` refuses:

- a NAME that is not `[A-Za-z0-9_-]{1,64}` (no separator, no `..`, no dot);
- a NAME equal, ignoring ASCII case (macOS and Windows fold case), to the
  first component of any compiled-in `lib/` entry, with or without its
  extension (`artkit`, `fonts`, `styles`, ...);
- any package file whose `lib/NAME/<rel>` equals a compiled-in name (belt and
  braces: the NAME rule already implies it);
- a destination that exists but was not made by `install` (it holds a
  `.pscat-lib` marker), so a hand-made directory is never replaced;
- a DIR that overlaps the install directory in either direction.

It copies regular files only: dotfiles and symbolic links are skipped with a
stderr note (a symlink in a plugin's directory could otherwise carry a file
from outside it into a directory the MCP server may read). More than 2000
files, 256 MiB, or a file over the resolver's 64 MiB `lib/` cap is refused.

*Copying while the source changes (Codex round 1).* The scan (`collect`) looks
at a file by path, so a concurrent writer could swap a file for a link, or grow
it, before the copy. The copy therefore does not reopen the path with
`fs::copy`: each file is opened once with `O_NOFOLLOW` (Windows: opened with
`FILE_FLAG_OPEN_REPARSE_POINT`, then refused if the handle carries the reparse
attribute), `O_NONBLOCK` so a swapped-in FIFO cannot hang the open, checked
regular on the handle, and read through `take(room + 1)` where `room` is the
smaller of the per-file cap and what is left of the total cap; reading past
`room` is an error. Residual: the check that no *directory* on the way was
swapped for a link (`canonicalize` of the parent must stay under the package)
runs just before the open, and std has no `openat`, so a swap in between is not
closed. The Windows branch is written to the documented flags but was not run.

*Scratch directories.* A re-install never merges: the message says how many files
the old install had and names the ones the package no longer holds. The new copy is built in `.NAME.new-<pid>` and the old
package is moved into `.NAME.old-<pid>/old`. Both directories are created
exclusively, so a directory of that name that this run did not make (a hand-made
one, or one left by a crash that a later process with the same pid trips over)
is refused with a message, never deleted; the `.pscat-lib` marker protection
cannot be bypassed through them. Only a directory this run created is removed.
A stale one is swept at the start of an install (#1344, `notes/1344-lib-install-stale-stage.md`).