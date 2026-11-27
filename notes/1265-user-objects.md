## User objects: defineuserobject, execuserobject, undefineuserobject (issue #1265, 2026-11-27)

The three Level 2 user-object operators, in `src/ops/level2.rs`. Their
semantics are Ghostscript 10.07.1's, taken from its own PostScript
definitions (`Resource/Init/gs_dps2.ps`) and pinned run for run in
`tests/user_objects.rs` (77 cases, run by pscat and by `gs`).

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
nothing. A growth is refused against `--max-memory` before the new array is
made (`check_alloc`) and charged to `--max-steps` and `--max-time` (one step
per 64 slots copied) before anything changes, so a limit stop leaves
`UserObjects` as it was. The first in-place write inside a `save` snapshots
the whole array (the journal's design), and is charged the same way. Worst
shapes are tests: growing by two a time up to 65535 slots
(`tests/user_objects_limits.rs`), a `save`/`restore` loop over the full
array, and the biggest growth under a small memory cap
(`tests/user_objects_memory.rs`, a binary of its own).

**Deliberate differences from gs.**
- An error *inside* the object (`0 {nosuchname} defineuserobject 0
  execuserobject`) blames the command that failed (`nosuchname`), as `exec`
  does; gs blames `execuserobject`, because there it is a PostScript
  procedure. The PLRM defines the operator as `userdict /UserObjects get
  exch get exec`. The error name is the same.
- `{ 0 execuserobject }` stored at 0 is a tail call, so running it never
  fills the execution stack: it loops until a step limit, like `/f { f }
  def f` in both interpreters. gs gives `execstackoverflow` for it because
  its return frame from the procedure is not a tail call. A body that does
  more after the call (`{ 0 execuserobject 1 }`) overflows in both.
- A `UserObjects` that a program replaced with a string or dict is
  `typecheck` here. gs reads a string through `get` (and `length` accepts a
  dict); nothing but a program overwriting the name can reach it.
- `noaccess` and `executeonly` are not modelled on arrays (#1189), so
  executing from such an array is not `invalidaccess` as in gs.

**Deferred.** None needed by a user; these are the PLRM's three operators
and the array they use.
