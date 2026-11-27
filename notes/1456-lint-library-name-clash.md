# #1456: `--lint` says when a program's own name shadows one a library defines

Follow-up to #1225 (`notes/1225-library-helpers-by-name.md`): libraries now reach their
helpers by `//name exec`, but a program can still hand a library a name it reads by bare
name (data and state names, forward references, public brushes, a user's own library), and
the error then reads like a mistake in the program's own code. Before #1453 the issue's own
program (`2 dict begin /shade [0.2 0.3 0.5] def ... pkliner end`) ended in

```
%%[ Error: typecheck; OffendingCommand: setrgbcolor; Line: 4 ]%%
  in a procedure, line 5228 (lib/paintkit.ps): ... pqbase 1 pqtint --add-- shade >>> --setrgbcolor-- <<< ...
```

and `--lint` said nothing about `shade`.

## The choice: an error-time note

The issue offered two designs: a note only when an error is reported, or a low-noise note in
`pscat lint` whenever a program defines a name a loaded library defines. The sweep
decided it. The interpreter was run with a throwaway dump of "names a library defines in
`userdict` that the program also defined" over `lib/`, `gallery/` and `examples/`
(194 files, 124 of which load a library):

| program | name | library |
| --- | --- | --- |
| `gallery/varsity_backs.ps` | `shade` | `lib/artkit.ps` (its own `shade` helper) |
| `examples/printkit.ps`, `stippling.ps`, `surfacekit.ps` | `showctr` | `lib/artkit.ps` |
| `examples/style_scifi.ps` | `sfworld` | `lib/styles/scifi.ps` (a hook the program sets) |
| `examples/style_steampunk.ps` | `spmetal` | `lib/styles/steampunk.ps` (a hook the program sets) |
| `examples/functional/observable-press.ps` | `ty.max-height` | `examples/functional/type-common.ps` |

7 of 124 library-loading programs (5.6%), every one correct and working: the programs
override a hook on purpose or keep a helper under the library's name. A static note would
fire on seven tracked programs and break the lint-clean gate, so each would need a
`%lint: allow` line for a thing that is not wrong. It is rejected. (A token-level
Python scan over the same files, which also counts the names libraries keep in private
dictionaries, such as the turtle's `tx`, said 10 files; the interpreter-level count is the
one that matters.)

`OffendingCommand` is not a separate rule. The issue's wording was "an error's
OffendingCommand is that name", but the issue's own program ended in
`OffendingCommand: setrgbcolor`, three elements after a library element `shade`, so that rule would
have missed the motivating case. A name that is the offending command and was run inside a
library procedure is in that procedure's excerpt already; one run from the program's own text
is the program calling its own name, and a library-defined name would be a false claim there.

The error-time note adds nothing to a run that works: it is made only when the run ended in
an error, and then only for a name a library procedure read by name where the error
happened. The sweep over the same files gives output identical to the base's (numbers in the
PR).

## What it says

```
pscat: lint: [shadowed-library-name] `shade` is defined at line 4 of your program and also by lib/artkit.ps, and a library procedure reads `shade` by name where the error happened, so it may have found yours (or yours has replaced the library's): rename yours to a name the library does not define, or, where your own code means the library's procedure, write `//shade exec`, which is bound when the program is scanned so a dictionary pushed later cannot intercept it
```

- `defined at line N of your program`, or `earlier in your program, before the library was
  loaded` when the program's definition came before the library's (the library's replaced it:
  the line is not known).
- The file is the library that defined the name first, from the loaded libraries'
  own definitions: a table the interpreter keeps while it runs, not a list in the source.
- It is in `ERROR_EXPLAINING_CHECKS`, so `--lint` and `pscat-mcp`'s `split_lint_findings`
  keep it after the `run-error` line (as `[shadowed-operator]`), and
  `%lint: allow shadowed-library-name` silences it.
- At most 5 are shown, then one line counts the rest.

## How

All of it under `--lint` (`Interp::trace_operator_shadows`, which `Options::new_interp`
turns on); a plain run has no new branch, and a host (`pscat::host`, psanim) never turns it
on.

1. **Library names.** `note_library_name`, called from `define_name` and `note_store_shadow`
   next to the operator-shadow note, records a name the first time a `run` file that is the
   innermost source (`last_file`) defines it in `userdict`: `lint::NameClashes`, a table
   from name id to the file and to what the program has defined of it. A name already in
   `userdict` then was put there by the program: `Before`.
2. **Program names.** A `def` or `store` outside any `run` file of a name in the table is
   classified once per program: the program's own `def` (the main scanner with a `def`/`store`
   token on top, a procedure frame whose previous element is the `def`, or
   `running_def_site`, #1338) gives its line; anything that is not found to be the program's
   (a library procedure that `def`s its own state while the program runs, a `def` in the tail
   position of a library procedure, a computed name) is not blamed. The place such a
   `def` was called from is remembered (the body's storage, the slot and the table
   generation), so a library procedure run in a loop is looked into once. It is made in
   any dictionary: #1225's case is a local `2 dict begin`.
3. **The error.** `snapshot_error_trace` (it runs only for an uncaught error, and it already
   walks the procedure frames for `$error`/`error_details`) keeps, for a lint run, the
   executable names in the elements it excerpts around each frame's running element, for the
   frames that were written in a `run` file (`BodyLines::line_of`, which knows `run` files only: the
   main program has a file name too under the CLI, and its own procedures are not a library's, test
   `the_programs_own_procedures_are_not_library_procedures`). At most 16 frames of 9 elements. These are the
   names the report's `in a procedure ...` lines show.
4. **The check.** `check_shadowed_library_names` reports each of those names that the
   table says the program also defined.

## Bounds

- At most `MAX_LIBRARY_NAMES` = 3584 names (the libraries in `lib/` define about 1300 distinct
  top-level names among them; the biggest program in the sweep tracked 673) and 64 files, in
  tables allocated whole when tracing starts (the same rule as `StackTrace`'s 1024 runs, so
  the memory cap sees them then and a definition never grows one). Past the cap a name is not
  followed. Nothing is quadratic in the number of definitions: a `def` costs a hash probe
  (id keys, one multiply), and the classification of a name that is in the table is once per
  program or once per call site.
- `begin_source` clears what the program defined, in a pass over the table, so the next
  program on the same interpreter is not blamed for the last one's lines.

## Cost

`--release` binaries, base = `origin/feature/0.8.0` at `6e24f61e`, head = this branch,
`/usr/bin/time -l` instructions retired, as `notes/1366-stack-leak-own-line.md`:

| program | mode | base | head |
| --- | --- | --- | --- |
| 13.5M-token flat file | plain | 17 760M | 17 758M |
| same | `--lint` | 21 306M | 21 318M |
| `0 1 3000000 { /x exch def } for` | plain | 3 405M | 3 413M |
| same | `--lint` | 4 112M | 4 217M (+2.6%) |
| `/f { 1 pop } def 0 1 10000000 { pop f } for` | plain | 12 182M | 12 182M |
| same | `--lint` | 14 561M | 14 564M |
| turtle: `0.5 fd 1 tl` x 400000 (artkit) | plain | 3 340M | 3 343M |
| same | `--lint` | 3 942M | 4 030M (+2.2%) |
| library `bump` (`/tally ... def`, a library name) called 1M times | plain | 2 386M | 2 389M |
| same | `--lint` | 2 803M | 2 923M (+4.3%) |
| program `/tally exch def` (a library name) 3M times | plain | 3 405M | 3 414M |
| same | `--lint` | 4 115M | 4 309M (+4.7%) |

The plain rows move by noise in the run-to-run range (plain runs have no new code on the
step path). Before the call-site memo the library-procedure row was +37% (every `def` ran
`running_def_site`).

## Not done

- A name written with `put` into a dictionary, or a name a library procedure reads more than 5 elements before or 3 after the
  running one (the window the report's context lines show), is not seen. A program that defines the name before a library does *and* never again
  gets the `Before` wording, without a line.
- Names a library keeps in a private dictionary (`TurtleState`'s `tx`) are not tracked:
  only `userdict` names are, since a program cannot collide with a closed dictionary.
- A top-level `stop` as the error (`take_top_level_stop_error`) does not pass the place the
  names are kept; an uncaught error from an operator, `error` and the limits do.
- `pscat lint` for a program that works says nothing: the sweep above is why.

## Compatibility

`lint::LintFinding`, the existing check names (`shadowed-operator`, `stack-leak`,
`utf8-in-byte-font`, ...) and their messages are unchanged; `lint::ERROR_EXPLAINING_CHECKS`
has one more entry and `LintAllows` one more private field. Nothing in `pscat::host` reads
this finding, and `pscat::host` never turns `--lint`'s tracing on, so psanim's findings
([utf8-in-byte-font]) and the host findings shape are as before. `pscat-mcp` keeps the new
finding after an error, under `Lint:`, as it does `[shadowed-operator]`.
