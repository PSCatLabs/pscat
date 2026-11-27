## Operand restore leftovers: exec of a name at the execution-stack limit, and image blame after `image` is redefined (issue #1242, 2026-11-27)

Two crafted-input diagnostics left over from the operand ledger (#1047, PR
#1237 round 2). Both reproduced on `feature/0.8.0` and both are pinned from
gs 10.07.1 in `tests/operand_restore.rs`.

**`exec` of an executable name on a full execution stack.** `exec_room`
asked only whether the operand itself was a non-empty procedure, a string or
a file, so a name passed. `execute_resolved` then resolved it to a procedure,
took the operand and overflowed. `/r { /r cvx exec 0 pop } def { r } stopped`
left an empty stack where gs leaves `r`. `exec_room` now returns at once
while there is room (`if` and `ifelse` call it for every branch, so the hot
path is one compare), and on a full stack asks `would_push_frame`, a cold
read-only helper that follows the name chain as `execute_resolved` does (same
100-hop budget, no write to `last_name`, no allocation) and answers whether a
frame would be pushed. An undefined name, an operator, a `noaccess` string or
file and an over-long chain say no, so those errors are raised by execution
exactly as before; nothing is popped on the overflow, so the operand stack
is unchanged by it.

**Image blame looked the operator up again.** `ImageCtx` kept the interned
name of the operator that began the image and `blame_image` loaded it through
the current dictionaries, so after `/image /pop load def`, bad data in a
procedure bound earlier blamed `--pop--` (gs: `--image--`). The frame now
holds the `Operator` (name and function, `Copy`), and `blame_image` builds
the object from it. `begin_image` takes the `Operator`; the dict forms
(`ImageType 3`, `imagemask` with a dict) pass theirs through, and the kernel's
`kernelimage` gives its own, which the old lookup also named.
