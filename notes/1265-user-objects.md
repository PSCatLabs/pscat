## User objects: defineuserobject, execuserobject, undefineuserobject (issue #1265, 2026-11-27)

The three Level 2 user-object operators, in `src/ops/level2.rs`. Their
semantics are Ghostscript 10.07.1's, taken from its own PostScript
definitions (`Resource/Init/gs_dps2.ps`) and pinned run for run in
`tests/user_objects.rs` (86 cases, run by pscat and by `gs`).

**What they do.** `UserObjects` is an array in `userdict` (always the
permanent userdict, whatever dictionary is open on top).

- `index obj defineuserobject`: an index over 65535 is `rangecheck`. The
  first call makes the array with `max(index + 1, 10)` slots. Later, an
  index at or past the length makes a new array, `2 * index` slots when the
  index equals the length and `index + 1` otherwise (gs's "apparent Adobe
  growth": lengths 10, 20, 20, 40, 101, 65536 for the pinned sequence), copies
  the old slots over and puts it back in userdict; an index inside the array
  writes in place. Operand errors leave the operands on the stack.
- `index execuserobject`: runs the slot as `exec` does (a procedure runs, a
  literal is pushed, an empty slot is `null` and is pushed). A non-integer
  index is `typecheck` before `UserObjects` is looked at; no array is
  `undefined`; an index outside it is `rangecheck`.
- `index undefineuserobject`: puts `null` in the slot (the array does not
  shrink); same errors.

**Quirks kept because gs has them.** A negative index with no array yet
makes the ten-slot array and then fails with `rangecheck`; a real index
(`3.0`) is `typecheck`, after the array was made when `index + 1` is at
most 10; an empty `UserObjects` and index 0 replaces it with another empty
array and fails `rangecheck`; a read-only `UserObjects` refuses an in-range
define or undefine (`invalidaccess`) but a define past its end grows into a
fresh writable array.

**Save and restore.** `UserObjects` lives in VM: a `restore` takes back an
array first made after the `save`, a growth, and in-place writes and
undefines (the array is journalled like any array, userdict like any dict).

**Limits.** The 65535 cap bounds the array at 131070 slots, so
`1000000000 {} defineuserobject` is a plain `rangecheck` that allocates
nothing. Every refusal comes from one preflight that runs before anything
is made, mutated or installed, so `UserObjects`, userdict and (through the
operand ledger) the operand stack are as they were after a limit stop
(Codex round 1, L1-L5):
- A growth or first creation asks `--max-memory` for the new array and for
  the first snapshot of userdict that installing it makes inside a `save`
  (`Interp::dict_snapshot_slots`), and charges one step per 64 slots
  initialised or copied plus one per 64 snapshot slots.
- The array is built at its final capacity in one allocation, so the check
  is for what is allocated (a copy followed by a resize can take twice).
- An in-place write or undefine inside a `save` is charged the snapshot
  `journal_array` makes, which is the whole *backing store* whatever small
  view `UserObjects` is (`Interp::array_snapshot_slots`), and only when
  that store has not been journalled at the current save level: 100 writes
  in one save cost one snapshot.
Worst shapes are tests: growing by two at a time up to 65535 slots
(`tests/user_objects_limits.rs`), a `save`/`restore` loop over the full
array, a one-slot view over 655350 slots, a 100000-name userdict, and the
growth and snapshot cases under a small memory cap with the state read back
(`tests/user_objects_memory.rs`, a binary of its own).

**Deliberate differences from gs** (each run on gs 10.07.1; pinned for
pscat in `tests/user_objects.rs`).
- An error *inside* the object (`0 {nosuchname} defineuserobject 0
  execuserobject`) blames the command that failed (`nosuchname`), as `exec`
  does; gs blames `execuserobject`, because there it is a PostScript
  procedure. The PLRM defines the operator as `userdict /UserObjects get
  exch get exec`. The error name is the same.
- The operand stack after such an error is what the failing operator
  left, as for `exec`. `0 { 1 0 idiv } defineuserobject { 5 0
  execuserobject } stopped pop` leaves `5 1 0` (count 3) here and `5 1`
  (count 2) in gs; `{ 10 20 (x) add }` leaves `5 10 20 (x)` here and `5 10`
  in gs. A plain `5 1 0 idiv` leaves `5 1 0` in both: gs's procedure
  wrapper unwinds to its own entry.
- `{ 0 execuserobject }` stored at 0 is a tail call, so running it never
  fills the execution stack: it loops until a step limit, like `/f { f }
  def f` in both interpreters. gs gives `execstackoverflow` for it because
  its return frame from the procedure is not a tail call. A body that does
  more after the call (`{ 0 execuserobject 1 }`) overflows in both.
- A `UserObjects` that a program replaced with a string or dict is
  `typecheck` here for all three operators. gs reads and runs through
  `get`: `userdict /UserObjects (abc) put 1 execuserobject` is 98 and
  `<< 1 33 >>` gives 33; `1 undefineuserobject` on the dict gives null and
  on the string `typecheck`, and `1 (x) defineuserobject` is `typecheck` on
  both. Nothing but a program overwriting the name reaches it.
- `noaccess` and `executeonly` are not modelled on arrays (#1189): `userdict
  /UserObjects 3 array noaccess put 0 execuserobject` runs (pushes null)
  here and is `invalidaccess` in gs.
- An executable null in a slot (`0 null cvx defineuserobject 0
  execuserobject`) is pushed here (count 1) and does nothing in gs (count
  0): inherited from plain `exec` (`0 null cvx exec` pushes null here).
- Pinned as matching gs: replacing `UserObjects` while a stored object runs
  keeps the running object (`0 {/UserObjects [99] def 42} defineuserobject
  0 execuserobject 0 execuserobject` leaves `99 42`); a real index into a
  read-only array is `invalidaccess` below the length (`0.0`, `-1.0`,
  `2.0` on three slots) and `typecheck` at or past it (`3.0`).

**Deferred.** None needed by a user; these are the PLRM's three operators
and the array they use.
