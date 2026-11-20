## `currentfilename` and Windows separators (issue #1346, 2026-11-20)

Follow-up to #1056 (deferred item "Windows-style `\` separators").

**Decision: the helper accepts both separators; `currentfilename` is unchanged.**
The operator's contract is "the name as it was given" (the same string an error
report's `Line:` prints), and on Unix `\` is a legal file-name character, so
rewriting separators inside the operator would either differ by platform or
corrupt a Unix name. The documented `dirname` is PostScript in
`docs/guides/getting-started.md`; it now compares the byte to both 47 (`/`) and
92 (`\`), so a name such as `vendor\pkg\main.ps` (a Windows command line) or a
mixed `C:\x/y\z.ps` splits at its last separator. A `\` inside a Unix directory
name would also split there, which is the right reading for a path that arrives
from Windows and a corner case nobody writes on Unix.

Tests (`tests/currentfilename.rs`): `dirname_splits_on_either_separator` runs
the helper on every platform (no files needed); a `#[cfg(windows)]` test runs a
main program given as `vendor\pkg\main.ps` and loads its sibling, so the
Windows CI legs cover the real command line. That test is not run locally;
the Windows CI leg runs it.
