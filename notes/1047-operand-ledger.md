## A failed operator leaves its operands; errors name the operator (issues #1047, #1074, #1198, 2026-11-16)

**Bug.** When an operator failed, pscat popped some or all of its operands
(`{ 1 (a) add } stopped` left `1` where gs leaves `1 (a)`), sometimes the
caller's values too (`77 { 1 2 rectfill } stopped` lost the `77`; a
`stackunderflow` in `astore` emptied the stack). `$error /command` and the
report's `OffendingCommand` named the enclosing bound procedure, or the last
local looked up, not the operator (#1074: `/f { ... curveto } bind def f`
reported `f`).

**What changed.**
- *The operand ledger* (`src/interp.rs`, `run_op`). Every operator dispatch
  sets `op_low`, the depth at dispatch. `pop`, `pop_num` and `pop_int` record
  each value they take from under it: numbers and booleans as their bits and a
  tag in fixed slots (`op_bits`/`op_tags`, 8), anything else, and anything past
  the slots, as a copy in `op_objs`. On success the ledger is emptied (one
  store unless it holds copies); on an error the stack is cut back to `op_low`
  and the values pushed back, so whatever the operator pushed goes and its
  caller's operands return. `op_low` is 0 outside an operator, so a host's
  `Interp::pop` records nothing, and it is saved and put back around each
  dispatch, so an operator run from inside another (`/add load exec`) has a
  ledger of its own.
- *Handing an operand on.* `exec_object` (behind `exec`, `if`, `ifelse`,
  `stopped`, the kernel) spends the ledger first: an error in what runs is its
  own, and the outer operator's operand has gone to the execution stack, as in
  gs (`5 { (x) add } exec` leaves `5 (x)`). The errordict handlers consume
  their operand (gs: `5 errordict /rangecheck get exec` leaves nothing). The
  kernel's fallback, which puts its caller's operands back itself before
  failing, calls `keep_operand_stack`.
- *Blame.* A failed operator becomes `$error /command` (the operator object,
  as gs stores it) and `OffendingCommand`, unless something more specific is
  there already (an errordict handler's object, a name `exec` did not find,
  `image`'s own blame). A procedure element's step carries only the function
  (see Performance), so `operator_name` finds the name: in the procedure on
  top of the execution stack, else through the last name run, else in
  systemdict by function address. `cvs` of an operator gives its name (PLRM,
  gs), and the report and the re-raise read `/command` through it.
- *Local fixes.* `astore` checks the depth before it writes (gs leaves the
  array untouched). An image data procedure's non-string result, and a
  Separation tint transform's non-number one, stay on the stack. `if` and `ifelse` require procedures, in gs's check order (the
  procedure's type, the depth, the boolean): `true 5 if` was running `5`.

**Performance.** The first designs copied each popped `Object` (40 bytes) to
a ledger. Every variant of that (inline, out of line, fixed slots, deferred
truncation) cost 13-25% more instructions and 27-50% more CPU on step-heavy
loops: the copy and its drop check bloat `pop`/`pop_num` until they stop
inlining into `moveto` and the like. Carrying the operator's name in
`ProcStep` cost 15% by itself, unused. What shipped keeps numbers as bits,
copies only non-numbers, and gives the hottest operators a check-then-pop
fast path that skips the ledger (`pop_settled`): `def` with a name key, `get`
and `put` on an array index or a dictionary name, `known`, `if`, `ifelse`,
`exec`; `and`/`or`/`xor`/`not`, the rounding operators, the unary real
operators and `cvi` compute in place. Instructions retired (macOS
`/usr/bin/time -l`), base `cc661d3` against this branch:

| program | base | branch | change |
|---|---|---|---|
| `examples/fillkit.ps` | 34.338 G | 34.517 G | +0.5% |
| `gallery/chalkboard_lecture.ps` | 85.855 G | 85.593 G | -0.3% |
| `gallery/fern.ps` | 3.667 G | 3.683 G | +0.4% |
| `gallery/frost_mandala.ps` | 514.3 M | 516.3 M | +0.4% |
| number-operator loop (moveto, lineto, cvi, atan; 2M iterations) | 8.350 G | 8.329 G | -0.3% |
| mixed loop (def, get, exch, copy, cvi; 3M iterations) | 14.775 G | 14.520 G | -1.7% |

CPU time, best of 9 on a loaded machine (load average 9-13): fillkit 1.699 s
base, 1.677 s branch; the mixed loop 0.676 s, 0.643 s; the number loop 0.389 s,
0.413 s (best of 15; its fewest cycles over five runs are 1.516 G and 1.521 G,
so that gap is the machine, not more work).

**Limit errors.** `limitexceeded` (steps, time, memory) ends the run and is
never catchable; the restore runs for it the same way, so `--pstack-on-error`
and a host reading the stack see the operator's operands. A restore pushes
back only values that were on the stack moments before, into capacity the
stack already has, and allocates nothing (`op_failed` takes the copies off
the end of `op_objs`); `op_objs` gives back what one huge pop asked for once
it is emptied.

**Deviations kept** (each probed in both; gs 10.07.1 quoted):

| case | gs | pscat |
|---|---|---|
| error inside an image data procedure, `{ 1 (x) add }` | `77 1 (x) true cmd=--image--` | `77 1 (x) true cmd=--add--`: the operator that failed, as inside any other procedure |
| Separation tint transform returns `(x)` | `77 [4] -mark- null null null (x) true cmd=/--@anonymous_operator--` (gs's own internals) | `77 (x) true cmd=pop`: the bad result stays; the command is the last name the transform ran |
| `<< /PageSize 5 >> setpagedevice` | `77 false true` (gs's PostScript-level `setpagedevice` leaves a flag) | `77 -dict- true`: the operand comes back |
| `(Times-Roman) {..} 100 string /Font resourceforall` | runs the procedure | `undefined`: pscat has no `resourceforall` (unchanged) |
| `cshow`'s width operands | `6.67188` | `6.673828125`: real formatting and glyph width, not this change; the table's `cshow` row converts with `cvi` |

`findfont` of a non-name substitutes a font in both (no error). Two
operators registered under one function would be named by whichever
systemdict finds first, only for a tail-call failure in a bound procedure.

**Verified against.** Ghostscript 10.07.1: `tests/operand_restore.rs` pins 118
cases (operator families × error kinds, stack contents and `$error /command`)
and runs them through `gs` when it is installed.

**Not done.** A handler a program installs in `errordict` is still not run on
a real error. Operators that change the stack directly were audited, not
moved onto the ledger: each checks before it changes anything.
