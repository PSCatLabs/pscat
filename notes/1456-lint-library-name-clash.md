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
library procedure is in that procedure's context lines already; one run from the program's own
text is the program calling its own name, and a library-defined name would be a false claim there.

## Round 1 of the review (Codex, PR #1479): make it precise or silent

The first version recorded when the program defined a name and reported every such name found
in the context lines of an error. Codex's programs showed it explaining errors wrongly too often:
names after the failing element that never ran (F1), deliberate hooks (`sfworld`, `spmetal`,
`ty.max-height`) blamed for unrelated failures (F2), definitions it missed (before the library
loaded, by a callback: F3, F4), ended or restored definitions still blamed (F5), stale files and
lines (F6), a classification cache that outlived a rebinding (F7) and advice that does not fix the
case (F8). A wrong explanation is worse than none, so the rule is now decided from what is true
when the finding is made, not from a record of what was done.

A name is reported only when all of these hold:

1. **A library defined it.** A `run` file defined the name in `userdict` while it was the
   innermost source (`last_file`); the table keeps the file (the latest definition, so a `restore`
   and a second library are followed, F6) and the *kind* of what it defined.
2. **A library procedure ran it before the error.** At the uncaught error `snapshot_error_trace`
   keeps the executable names at and before the running element of each procedure frame
   written in a `run` file (not the ones after it, which had not run: F1). It is the frames'
   own history, so no per-step record is made.
3. **It is in force now and of another kind.** The name is looked up through the dictionary
   stack as the error left it, and its binding must differ in kind from the library's
   (procedure, array, number, name, other type). That settles ended `begin`s, `restore`s,
   definitions made before the library loaded or by a callback, and definitions the library then
   replaced (F3, F4, F5): none of them needs tracking, the binding is looked at where it lives.
   A hook set to a value of the same kind (a number by a number, a procedure by a procedure, a
   name by a name: `showctr`, `sfworld`, `spmetal`, `ty.max-height`, `pal`) is deliberate and
   silent (F2). An operator is never a program's binding.

The one thing noted while the program runs is the *line* of the last `def` or `store` of a table
name that is certainly in the program's own text: the main scanner with a `def` token on top, or a
procedure frame whose previous element is a `def` spelling `/name`, in a body the table of lines
says is the program's. Any other `def` (a tail position whose frame is gone, a callback, a call
through a name) sets the line unknown and the message says no line (a wrong line is worse than
none, F6). A `def` found in a library's procedure while the program runs updates the kind the
library has now, so a library that changes its own state's kind is not blamed. There is no
classification cache any more (F7): the expensive `running_def_site` fallback is gone, and every
`def` of a table name looks at the frame on top, at constant cost.

## What it says

```
pscat: lint: [shadowed-library-name] `shade` is a procedure in lib/artkit.ps, but your program's `shade` (set at line 4) is now an array, and a library procedure ran `shade` before the error, so it may have used yours instead of the library's. If the error looks unrelated to your code, that is why: rename yours to a name the library does not define. (`//shade` captures the binding visible when the program is scanned, so it helps your own code only if it is scanned before yours is defined, and it cannot recover a library definition you replaced.)
```

- `(set at line N)` only when the last def is certainly in the program's text.
- The file is the library that defined the name last, from the loaded libraries' own
  definitions: a table the interpreter keeps while it runs, not a list in the source.
- The advice is true and hedged (F8): the only fix a user can make for a library that reads a
  name by bare name is renaming; `//name` binds what is visible at scan time, so `//helper exec`
  scanned after the program's own definition gets the program's value (test
  `the_advice_is_true_and_hedged` runs it). notes/1225 says the same: `//name` cannot recover an
  overwritten library definition.
- It is in `ERROR_EXPLAINING_CHECKS`, so `--lint` and `pscat-mcp`'s `split_lint_findings`
  keep it after the `run-error` line (as `[shadowed-operator]`), and
  `%lint: allow shadowed-library-name` silences it.
- At most 5 are shown, then one line counts the rest.

## How

All of it under `--lint` (`Interp::trace_operator_shadows`, which `Options::new_interp`
turns on); a plain run has no new branch, and a host (`pscat::host`, psanim) never turns it
on.

1. `note_library_name`, called from `define_name` and `note_store_shadow` next to the
   operator-shadow note, fills `lint::NameClashes` (name id to file, kind and line) as above.
   `def_origin` tells whose `def` is running from the frame on top of the execution stack.
2. `snapshot_error_trace` (only for an uncaught error; it already walks the procedure frames for
   `$error`/`error_details`) keeps the names run by frames written in a `run` file
   (`BodyLines::line_of`, which knows `run` files only: the main program has a file name too under
   the CLI, and its own procedures are not a library's, test
   `the_programs_own_procedures_are_not_library_procedures`). At most 16 frames of 6 elements.
3. `check_shadowed_library_names` looks each name up through the dictionary stack and reports
   the ones of another kind than the library's.

## Bounds

- At most `MAX_LIBRARY_NAMES` = 3584 names (the libraries in `lib/` define about 1300 distinct
  top-level names among them; the biggest program in the sweep tracked 673) and 64 files, in
  tables allocated whole when tracing starts, a fixed overhead of about 70 KiB, so a definition
  never grows one. That allocation is **not** charged to `--max-memory`: the CLI starts the tracing
  (`Options::new_interp`) before it applies the limits, so it is part of the baseline, not of the
  program's growth (F10). Past either cap a name is not followed: the finding is **silently** not
  made for it (a false negative). Nothing is quadratic in the number of definitions: a `def` costs a
  hash probe (id keys, one multiply), and for a name in the table a look at the frame on top.
- `begin_source` clears the lines (a pass over the table), so the next program on the same
  interpreter is not given the last one's lines.

## Cost

`--release` binaries, base = `origin/feature/0.8.0` at `6e24f61e`, head = this branch,
`/usr/bin/time -l` instructions retired, as `notes/1366-stack-leak-own-line.md`:

| program | mode | base | head |
| --- | --- | --- | --- |
| 13.5M-token flat file | plain | 17 768M | 17 749M |
| same | `--lint` | 21 148M | 21 149M |
| `0 1 3000000 { /x exch def } for` | plain | 3 404M | 3 413M |
| same | `--lint` | 4 112M | 4 217M (+2.6%) |
| `/f { 1 pop } def 0 1 10000000 { pop f } for` | plain | 12 178M | 12 177M |
| same | `--lint` | 14 559M | 14 558M |
| turtle: `0.5 fd 1 tl` x 400000 (artkit) | plain | 3 341M | 3 344M |
| same | `--lint` | 3 943M | 4 035M (+2.3%) |
| library `bump` (`/tally ... def`, a library name) called 1M times | plain | 2 385M | 2 388M |
| same | `--lint` | 2 790M | 2 923M (+4.8%) |
| program `/tally exch def` (a library name) 3M times | plain | 3 404M | 3 413M |
| same | `--lint` | 4 112M | 4 512M (+9.7%) |

The plain rows move by noise (the base's own run-to-run spread is about 10M on 3 400M; plain
runs have no new code on the step path). The last row is the worst case, a program that `def`s a
library's name three million times: each `def` looks at the frame on top and at the table of
lines.

## Not done

- A name written with `put` into a dictionary, or `/helper load exec`, is not seen as a program's
  binding by the line (the lookup at report time sees the binding wherever it lives, so a `put`
  that replaced it is still reported, without a line).
- A name a library procedure ran more than 5 elements before the running one (the window the
  report's context lines show) is not seen.
- A library procedure that `def`s its own state in tail position (its frame gone) is not told
  apart from a program's: if it changes the state's *kind* it can be blamed on the program.
- Names a library keeps in a private dictionary (`TurtleState`'s `tx`) are not tracked: only
  `userdict` names are.
- A library loaded inside a local `begin` (its names go into that dictionary, not `userdict`) is
  not followed.
- A second program on one interpreter has its source positions cleared when tracing is on
  (`begin_source`), so a library loaded by an earlier program is not recognised as one in the
  next; a program that loads the library itself is (test
  `a_second_program_that_defines_the_name_again_is_named_again`).
- A top-level `stop` as the error (`take_top_level_stop_error`) does not pass the place the
  names are kept; an uncaught error from an operator, `error` and the limits do.
- `pscat lint` for a program that works says nothing: the sweep above is why.

## Compatibility

`lint::LintFinding`, the existing check names (`shadowed-operator`, `stack-leak`,
`utf8-in-byte-font`, ...) and their messages are unchanged. Two additive changes are visible to
code that matches on them: the public constant `lint::ERROR_EXPLAINING_CHECKS` has one more entry,
so `lint::explains_an_error("shadowed-library-name")` is true (F9), and the private `LintAllows`
has one more field. Nothing in `pscat::host` reads this finding, and `pscat::host` never turns
`--lint`'s tracing on, so psanim's findings ([utf8-in-byte-font]) and the host findings shape are
as before. `pscat-mcp` keeps the new finding after an error, under `Lint:`, as it does
`[shadowed-operator]`.
