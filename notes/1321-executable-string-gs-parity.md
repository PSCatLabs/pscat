## Executable strings: exit, noaccess and //name (issue #1321, 2026-11-19)

Follow-up to #1122 (notes/1122-executable-strings-in-procedures.md). Four differences from
Ghostscript 10.07.1 (`gs -q -dNODISPLAY -dNOSAFER -dBATCH -dNOPAUSE -c`) in how an executable
string or file is run. Every pinned program is in `tests/exec_strings.rs` with the gs output it
matches; the test runs gs too when it is installed.

1. **`exit` inside an executable string or file.** `1 { [ (exit) cvx ] cvx exec 2 } loop 3`:
   gs leaves `1 3`; pscat raised `invalidexit` because the string's scanner frame was a boundary.
   The scanner of a string or file run by `exec`, a name, or a procedure element now carries
   `Lexer::exit_through`, and `Interp::exit_loop` pops such a frame and keeps looking. Source
   boundaries stay: the main program, a `run` file (gs: `invalidexit` too, probed), an eexec
   stream, `stopped`, a show in progress. `(exit) cvx exec` with no loop is `invalidexit`, as in gs.
2. **`noaccess` string or file.** gs: `(7) cvx noaccess exec` is `invalidaccess` and leaves the
   string on the stack; `[ (7) cvx noaccess ] cvx exec` is `invalidaccess` and leaves nothing; a
   name defined as such a string raises it too; `readonly` and `executeonly` still run. `Object`
   gains a `noaccess` bool (set by the `noaccess` operator on a string or file, copied with the
   reference; it fits the padding beside `readonly` and `packed`), checked in `execute_resolved`'s
   string and file arms and, for the operand left behind, at the top of the `exec` operator.
   Nothing else reads it: a `noaccess` string still reads and prints as its text (gs prints
   `-string-` and refuses `get`/`getinterval`; not modelled, as for arrays since #1189).
   `noaccess` on an executable array still only write-protects it; gs also refuses to run it
   (`{7} noaccess exec` is `invalidaccess`).
3. **Tail recursion through a string** (`/f [ (f) cvx ] cvx def f`): gs loops forever, pscat stops
   with `execstackoverflow` (a string's scanner frame is not dropped before its last token runs).
   Left as a documented deviation, as the issue allows; pinned by a test so a change shows.
4. **`//s` whose value is an executable string** (or file), met by the scanner at the top level or
   in a string being run, now runs it as `{ //s }` already did when the procedure ran
   (`Interp::execute_element`). An executable array is still pushed (gs: `{1 2}`).
   **gs is unreliable for a multi-token string here**: `/s (7 8 add) cvx def //s pstack` prints
   `502392012220 7` (the first token runs, the rest is a garbage integer that changes from run to
   run), and `/s (/x 5 def) cvx def //s x` is `undefined`. The tests pin single-token strings
   (`(7)`, `(add)` with `1 2`, `(count)`), where gs and pscat agree, and pscat's own whole-string
   result (`15`) for the rest, which agrees with gs's `{ //s } exec` -> 15.
   One cosmetic difference: an error from running the string is blamed on the operator last run
   (`def`), where gs names the string.

Not done (proposed): `rcheck` and `wcheck` on a `noaccess` string or file (gs: false), refusing
`get`/`getinterval` on one, and running a `noaccess` array.
