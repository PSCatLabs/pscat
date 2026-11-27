## countexecstack, execstack and dictstack (issue #1468, 2026-11-27)

The three Level 1 stack-introspection operators, decided by running gs
10.07.1 and pinned in `tests/execstack.rs`: 54 `PARITY` cases that print
what gs prints, and 13 `OWN` cases where pscat differs on purpose, each with
gs's output beside it (both sides checked live when `gs` is installed; the
two gs-only tests run gs alone and so do not depend on the operators, every
other test failed at the base). `tests/data/plrm_operator_gaps.txt` no longer lists them.

**What `execstack` shows.** pscat's execution stack is a native `Frame`
vector, not PostScript objects, so each frame is mapped to what gs shows for
the same construct, bottom first (`frame_shape` / `frame_entries` in
`src/interp.rs`; one shape function, and the writer asserts it wrote that
many, so `countexecstack array execstack` always fits):

| frame | gs 10.07.1 | pscat |
|---|---|---|
| main program, `run` file, `exec`ed file, eexec | `-file-` (xcheck true) | the file, executable |
| executable string | `(== 0 pop)`, a view of its unread text; gone once its last token is read | a new executable string (a copy) of the unread text; gone once nothing is left |
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
the contexts the tests check match gs's differences exactly (procedure +1,
tail call 0, nested procedures, fused and plain `if`/`ifelse`, `bind`,
`exec`, array/string `forall` +4, `for` +6, `repeat` +4, `loop` +3, an
executable string +1 while text is left in it). Other contexts are not
claimed: see "Further differences" below for those measured. gs starts at
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

**Exhausted executable strings** (Codex round 1, S1). gs removes an
executable string as soon as its last token has been read: with nothing, or
the one space or newline that ends that token, after it,
`(countexecstack) cvx exec` counts as the top level and `(top) cvx exec`
shows the file beneath as the last entry. Two trailing spaces or a trailing
comment keep it (+1) in both. pscat shows a string scanner only while
`Lexer::with_unread` has text left, which gives the same answers for the
whole family (pinned: no trailing text, one space, one newline, two spaces,
a comment, a procedure read as the last token).

**String remainders are copies** (S2). The scanner runs an executable
string from a copy made when it starts (`s.to_vec()` in `execute_resolved`),
so the remainder `execstack` returns is a new string: writing into it does
not change the original, where gs's remainder is a view of it
(`string-rest-copy` pins both). A view would need the scanner to read from
the program's own `PsString` at an offset, and would also make a program
that rewrites a running string see its change, a wider semantic change than
this issue.

**The journal copy** (L1). Inside a `save`, the first write into an array at
that level journals its whole backing store, however short the view
(`journal_array`). `execstack` and `dictstack` now ask for that copy too
(`Interp::array_snapshot_slots`, as `ops::level2`'s `preflight` does for
#1459): its bytes with the string copies against `--max-memory`, a step per
64 slots with the per-entry steps, all before the operand is taken
(`ops::control::preflight_write`). A ten-slot view of a million-slot array
inside a `save` is now refused with the operand and the array untouched.
Each string copy is also counted with its shared cell's header
(`STRING_COPY_OVERHEAD`), not its bytes alone (N1).

**Further differences, measured in gs 10.07.1** (pinned in `OWN` and
`run_and_eexec_add_one_frame`; all follow from the skip rule above): an
`image` data procedure counts +1 here, +20 in gs; an `imagemask` one +1, +16;
a `PaintProc` +2 (its `Pattern` frame is skipped, the retry procedure
`{--rectfill--}` under it shows), +9 in gs; a `run` file +1, +25; an
`eexec` stream +1, +6. gs runs these through PostScript procedures and
`stopped` contexts of its own.

**Behaviour these operators expose but do not change** (S4, not fixed here):
- `1 dict cvx begin ... dictstack` gives the dictionary literal: `begin` keeps
  only the dictionary, not the object's executable attribute (`currentdict
  xcheck` is false too; gs true).
- `true setpacking` makes ordinary arrays (`arraytype`; gs `packedarraytype`,
  read-only), so `execstack` writes into one where gs raises `invalidaccess`.
- A procedure remainder of a `bind`ed procedure shows the operator where gs
  shows the name: that is what `bind` stores in pscat.
- Global VM is not tracked (#1146), so a `true setglobal` array is filled
  where gs raises `invalidaccess` for storing local objects in it.
- The dictionary stack has no depth limit: after 10,000 `begin`s `dictstack`
  returns 10,003 dictionaries, where gs has raised `dictstackoverflow`.
- A dict `forall` walks the snapshot it took, so a dictionary the body
  changes shows its new contents in `execstack` but is walked as it was.

