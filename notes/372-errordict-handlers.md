## errordict holds a handler per error name (issue #372, 2026-10-14)

**Bug.** `errordict` was an empty dict, so the re-raise idiom
`errordict /rangecheck get exec` failed on its own lookup: `get` of a missing
key is `undefined`, with OffendingCommand `rangecheck`. Ghostscript's
`errordict` has a handler per name; run, it raises that error again, so a
library that catches an error and re-raises it keeps `rangecheck`,
`typecheck` and the rest.

**Found by.** The #180 agent (PR #363): `pktrowelplane` catches a row's
error, undoes its clip and rotation, and raises it again. The idiom there is
guarded by `2 copy known`, so on pscat it never reached the missing handler;
it took the `stop` fallback instead, which the top-level `stop` turns back into
the right error from `$error` (#142). The issue as filed (a re-raise reporting
`undefined`) is the unguarded form, reproduced with `{ [1] 5 get } stopped {
errordict /rangecheck get exec } if`.

**What changed.**
- `src/ops/misc.rs` `install_errordict`: one handler per name the interpreter
  can raise (the 18 `PsError` variants) plus `handleerror`. A handler pops the
  top operand as the offending command (the PLRM has the interpreter push it
  before running a handler), stores it in `$error /command` as is (Ghostscript
  keeps `(abc)`, `/foo`, `2`), and raises the real error, so an enclosing
  `stopped` records the right `/errorname` and an uncaught one is reported
  under it.
- `src/interp.rs`: an `error_command` slot carries that object to
  `record_error`; `recover` empties it on every error, caught or not.
  `take_top_level_stop_error` reads `/command` of any type now.
- `lib/paintkit.ps` `pktrowelplane`: the re-raise now pushes `$error /command
  get` before the handler. Without it the handler takes whatever operand is on
  top: Ghostscript reported `5` (a restored operand of `get`) and pscat, now
  that the handler exists, `6.0`. With it both say `get`.
- Not added: Ghostscript's other 14 errordict names (`stackoverflow`,
  `VMerror`, `invalidaccess`, `configurationerror`, ...). pscat never raises
  them, so a program cannot catch one to re-raise.

**Deviation: an empty operand stack.** Ghostscript dies (an unrecoverable
`stackunderflow`, not catchable). pscat does not restore an operator's
operands when it fails (HANDOFF.md), so after a catch the stack is often
empty where Ghostscript's holds the failed operands. The handler then falls
back to the command `$error` already holds. That keeps the error name, which
is what a re-raise is for.

**Still not done.** A handler a program stores in `errordict` is not
consulted when an error happens (HANDOFF.md). Error-time operand restoration
likewise.

**Verified against.** Ghostscript 10.07.1: the handler pops its operand into
`$error /command`; names `/x` and `(abc)` keep their type; `pktrowelplane`
with a row error says `rangecheck` / `get` in both.
