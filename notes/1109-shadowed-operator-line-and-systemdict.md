# #1109: `[shadowed-operator]` gets a line, and sees `store` into systemdict

Follow-up to #1052 (`notes/1052-shadowed-operator-lint.md`), which listed three gaps.

## 1. Line numbers: done

`Interp::trace_operator_shadows()` (new, public) turns on a small trace; `--lint`
switches it on in `Options::new_interp`, so plain renders pay nothing and a `def`
pays one `Option` check. With it on, `define_name` and `store` note a binding of an
operator's name to a plain value together with the scanner's line (`last_line` /
`last_file`, the same source of lines the error report uses). One entry per name and
scope, the first line kept, so a loop redefining the name a million times holds one.
The end-state scan stays the source of truth for what is a shadow; the trace only
supplies the line.

Wording (the prefix `/sub was redefined as a string` is unchanged, and so is the whole
message when no trace ran, so `pscat-mcp` and the existing tests match as before):

- `/sub was redefined as a string when the program reached line 3, replacing the operator sub: ...`
- from a `run` file: `... when the program reached line 3 (lib/x.ps), ...`
- in systemdict: `/sub was redefined as an integer in systemdict when the program reached line 2, ...`

The interpreter tags no object with a source position, so the line is the one the
scanner had reached. For a top-level `def` that is the `def`'s own line. For a `def` inside
a procedure or loop body it is the line that ran it (a `for` loop reports its closing
`} for`; `gallery/hundred_lines.ps` says 261 for a `def` on 254). A procedure's last
element runs after the procedure's frame is gone, so the two cases cannot be told apart
at that point, and the wording ("when the program reached") claims no more than is true in
both. Pointing at the `def` inside a body would need a source position on each scanned
procedure element: a follow-up.

Cost: a plain render pays nothing. A `--lint` run (and so every `pscat-mcp` render, which
passes `--lint`) pays one systemdict probe per `def` into userdict or systemdict, plus a
clone and a map entry only when the name is an operator's. Measured with the release
binary, `0 1 3000000 { /x exch def } for`: 0.15 s with and without `--lint`; the same
loop on an operator's name (`/ln`, the worst case): 0.15 s plain, 0.19 s with `--lint`
(about 13 ns per `def`). A `def` made after `userdict begin` or `systemdict begin` is
classified by the dictionary itself, not by its place on the stack, so it keeps its line.

## 2. Closed local dictionaries: kept as the opt-out, on data

Other linters flag shadows in inner scopes (pylint `redefined-builtin`, ESLint
`no-shadow` with `builtinGlobals`), so the first version reported them. Linting every
file of `lib/`, `gallery/` and `examples/` (172 files) gave:

| rule | hits |
| --- | --- |
| userdict only (today) | 1: `gallery/hundred_lines.ps` (`/ln` loop index), a true positive |
| plus local dictionaries | 3: the same, plus `gallery/ring_of_type.ps` and `gallery/ring_unrolled.ps` (which runs it) |

Both new hits are the deliberate idiom: `ringoftype` does `begin /stop theta 357 sub def ...
stop lt ... end`, a variable named `stop` that never reaches the program's own names.
That is what the message already tells a user to do ("keep it in a local dictionary with
begin ... end"), and flagging it would contradict the lint's own advice and triple the
false-positive rate the issue asks to hold. A shadow in a local dictionary stays
invisible; a bug there is still caught if the name leaks to `userdict`.

## 3. `store` into systemdict: done

`store` binds in the topmost dictionary that already defines the name, which for an
operator nobody has redefined is `systemdict`: `/sub 5 store` replaces the operator for
the whole program. Ghostscript raises `invalidaccess` there (its systemdict is read-only),
pscat does not. The check now compares `systemdict` with the operators a fresh
interpreter installs (`ops::install_all` into a scratch dictionary) and reports any
that is now a plain value, so it holds for every caller (embedders without the trace,
`--bands`), and `systemdict /sub 5 put` is caught the same way. Message:
`/sub was redefined as an integer in systemdict when the program reached line 2,
replacing the operator sub for the whole program: ...`. The line comes from the trace of
`def` and `store`, so a `put` has none of its own.

Same slug, so `%lint: allow shadowed-operator` and `ERROR_EXPLAINING_CHECKS` cover it.

## Not done

- `(sub) 1 def` (a string key) and `put` are not traced. A trace entry is dropped only by
  a later procedure binding, not by `restore` or `undef`, so a shadow made again after one
  of those reports the line of the first binding (the finding itself is still right).
- Making pscat's systemdict read-only like gs is an interpreter decision, not lint's.
