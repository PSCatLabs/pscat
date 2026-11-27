## countexecstack, execstack and dictstack (issue #1468, 2026-11-27)

The three Level 1 stack-introspection operators, decided by running gs
10.07.1 and pinned in `tests/execstack.rs` (live gs checks when `gs` is
installed). `tests/data/plrm_operator_gaps.txt` no longer lists them.

**What `execstack` shows.** pscat's execution stack is a native `Frame`
vector, not PostScript objects, so each frame is mapped to what gs shows for
the same construct, bottom first (`frame_shape` / `frame_entries` in
`src/interp.rs`; one shape function, and the writer asserts it wrote that
many, so `countexecstack array execstack` always fits):

| frame | gs 10.07.1 | pscat |
|---|---|---|
| main program, `run` file, `exec`ed file, eexec | `-file-` (xcheck true) | the file, executable |
| executable string | `(== 0 pop)`, its unread text | a new executable string of the unread text |
| procedure | the rest after the running element, `{== 0 pop}` | a read-only executable view of the rest |
| `repeat` | `4 {body} --%repeat_continue--` | `4 {body} --repeat--` |
| `loop` | `{body} --%loop_continue--` | `{body} --loop--` |
| `for` | `3 2 9 {body} --%for_pos_int_continue--` (next, step, limit) | `3 2 9 {body} --for--` |
| array / string `forall` | `[2 3] {body} --%array_continue--` (the rest) | `[2 3] {body} --forall--` (the rest) |
| dict `forall` | `-dict- {body} 2 --%dict_continue--` | `-dict- {body} --forall--` |
| `pathforall` | `{m} {l} {c} {cp} (gs_path_enum) --%path_continue--` | `{m} {l} {c} {cp} --pathforall--` |
| `stopped` | `false 1 --%stopped_push--` | `--stopped--` |
| an operator | nothing (it is running) | nothing |

The `--%..._continue--` objects are gs-internal operators with no
PostScript name; the operator that made the frame stands in for each. The
`false 1` under gs's `stopped`, its dict-`forall` slot index and its path
enumerator are internal state and are dropped, so `stopped` adds 2 to
`countexecstack` (gs 4), dict `forall` 4 (gs 5) and `pathforall` 6 (gs 7);
the other contexts match gs's differences exactly (procedure +1, tail call
0, array/string `forall` +4, `for` +6, `repeat` +4, `loop` +3). gs starts at
38 at the top of a file (its own startup procedures); pscat at 1, so only
differences are comparable. A procedure that has issued its last element
is already gone in both (tail call), so `{ countexecstack } exec` equals the
top level.

**Skipped frames** (no PostScript form): a show (`Frame::Show`; its Type 3
glyph and `kshow` procedures run as ordinary frames above it, and show), an
image (`Frame::Image`; likewise its data procedures), the tint-transform
continuation (`PostOp`), a pattern's `PaintProc` boundary (`Pattern`; the
one-element retry procedure planted under it does show, as `{--fill--}`:
it is a real procedure frame) and the profiler's `ProfReturn` (it exists
only under `--profile`, which must not change what a program sees; tested).
`kernelimage`/`kernelarray` never compile a procedure that uses these
operators (they are not in the kernel subset), so it runs through the
interpreter's fallback driver, whose own PostScript frames show.

**Access.** gs gives a procedure view the procedure's own attributes (a
writable `{...}` from `def` has `wcheck` true). A `Frame::Proc` keeps only
the `PsArray`, so pscat makes every procedure, loop-body and `forall`-array
view read-only rather than hand out write access to a read-only procedure.
The result is the operand's subarray with the operand's attributes, as in
gs.

**`dictstack`** writes systemdict, globaldict, userdict, then what `begin`
pushed, as literal dictionaries (gs: systemdict `wcheck` false, the others
true, none executable; pscat agrees). pscat models a dictionary's write
access only for systemdict, so `3 dict readonly begin` shows `wcheck` true
on top where gs shows false; that is `currentdict`'s answer too, and a
follow-up, not this issue.

**Errors**, in gs's order with the operand left: `stackunderflow`,
`typecheck` (not an array), `rangecheck` (too short, even when also
read-only), `invalidaccess` (a read-only or `noaccess` array). gs also refuses a packed
array (always read-only there); pscat's `setpacking` procedures are ordinary
writable arrays (`{1 2 3} cvlit type` is `arraytype`), so they are written,
as `put` writes them: a general packed-array gap, not this operator's.

**Limits.** The walk is a step per frame, with the limits polled every 1024
frames (`countexecstack` too); `execstack` then holds its string copies
against `--max-memory` (`check_alloc`) and charges a step per entry before it
takes the operand, so a refusal or a stop leaves the operand and the array
untouched; only then is the array journalled and written, which cannot fail.
`dictstack` charges a step per dictionary the same way (the dictionary stack
has no depth limit). Tests: a 9,000-deep recursion under `--max-steps`, a
100,000-deep `begin` nest, thirty 1 MiB executable strings under
`--max-memory`, and a stop just at the operator's charge leaving the array
null.
