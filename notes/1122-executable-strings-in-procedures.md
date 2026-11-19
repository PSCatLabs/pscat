## Executable strings and files in an executing procedure (issue #1122, 2026-11-19)

Found as a `token`-on-a-binary-sequence difference (#733, Codex round 3), but the rule is
general PostScript: an executable string or file met as an element of an executable array that
is running is **run as source**, not pushed. pscat pushed it (`Interp::proc_step` only ran
executable names, operators and procedure literals), so

```
[ (8 9 add) cvx ] cvx exec        % pscat: (8 9 add)    gs: 17
```

The `<seq> token pop exec` case in #1122 is one instance: `token` returns an executable array
whose element is the executable string, and executing it should run the string.

**Source of truth.** No PLRM text is in the repo; PLRM section 3.5 (how the interpreter treats
each object type: executable strings and files execute, in the object stream or in a procedure
being executed) is cited from memory, as in the #733 notes. The evidence is Ghostscript 10.07.1,
run with `gs -q -dNODISPLAY -dNOSAFER -dBATCH`:

```
rd token pop exec                     -> 17   (rd: the sequence file of (8 9 add) cvx)
[ (8 9 add) cvx ] cvx exec            -> 17
[ (8 9 add) cvx ] cvx bind exec       -> 17
{ [ (8 9 add) cvx ] cvx exec } exec   -> 17
[ (8 9 add) cvx ] cvx 0 get           -> (8 9 add)        (reading does not run it)
[ () cvx ] cvx 5 exch exec            -> 5
[ ({9}) cvx ] cvx exec                -> {9}
3 { [ (7) cvx ] cvx exec } repeat     -> 7 7 7
[ g ] cvx exec   (g: executable file "1 2 add") -> 3
```

**Change.** `proc_step` returns the new `ProcStep::Src` for an executable string or file
(a third arm in the match it already does for executable objects: names and operators are
unchanged, so the hot path only pays for objects that were already off the fast arms).
`Interp::proc_source` takes the element, advances `pc`, pops the frame if it was the last
element (a tail call, as for names), and runs it with `execute_resolved`, the code `exec` uses.

**The RunSequence workaround is gone.** #733 round 1 appended an `exec` after each executable
string of a scanned sequence. With the rule in the procedure frame it is redundant, and left in
it would run the string's result a second time (`({9}) cvx` would leave 9). The frame the
scanner builds for a sequence now holds the decoded objects as they are.

**Scope.** Only procedure elements. A `//name` immediately evaluated in a token stream whose value
is an executable string still pushes it (not probed against gs; rare). `token` itself is unchanged.
