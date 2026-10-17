## `Drop for Interp` frees systemdict/userdict (issue #625, 2026-10-16)

Part of release epic #562, a 0.6.0 blocker. #565 made `Interp` the documented
public type and hid `break_permanent_dict_cycle`, so a library user building
one `Interp` per document leaked each one's whole dictionary graph (systemdict
holds strong `Rc`s to itself and to userdict).

**Change.** `impl Drop for Interp` takes systemdict's and userdict's contents out
and drops them after the `RefMut` is released (so a program's own
`userdict /me userdict put` is freed too). `break_permanent_dict_cycle` is deleted
(it was `#[doc(hidden)]`); its callers (`--sweep`, `--spool`, the selftest
runner) now just drop. `Dict::clear` went with it. `Object` still has no
`Drop` (the HANDOFF note: ~18% of fib).

**Proof.** `dropping_an_interp_frees_its_dictionaries` runs a program that
uses user dicts, a restored and an open `save`, a caught error, `findfont` and
`show`, and an open `systemdict begin`, keeps `Weak` handles to seven dicts,
drops the `Interp` and asserts every `upgrade()` is `None`, twenty times.
With the `Drop` body disabled it fails on systemdict, so it discriminates.

**Rendering.** Teardown runs after all output is written, so nothing
rendered can change; there is no control that renders differently, so the
two-binary identity check has nothing to prove here. The golden tests and
selftest pass unchanged.

**Exit cost.** Freeing the graph at the end of a CLI run is wasted work: a
program with 600k dictionary entries took +80 ms (0.18 s to 0.25 s CPU) with a
plain Drop. `main` calls the hidden `Interp::leave_dicts_to_process_exit` on
its single-run `Interp`, which makes `Drop` skip emptying the dictionaries
and nothing else, so the CLI's exit path is exactly what it was (dicts left
to the OS, every other field dropped). With it the same program is back to
0.18 s; `gallery/fern.ps` shows no difference either way. The sweep, spool
and selftest loops, the library, MCP and wasm do free their dictionaries.
(MCP and wasm replace their `Interp` per request and never called the old
method, so they leaked per render; they no longer do.)

**Deep nesting (Codex review, rounds 1 to 3).** A first version let the
compiler's drop glue free the contents, which recurses once per nesting level
of dict (only `PsArray` drops iteratively): about 10000 levels overflow a 2 MB
thread, and for library, MCP and wasm callers that is a new input-triggered
abort where dropping used to leak. Rounds 1 and 2 fed a worklist from
systemdict, userdict, the operand stack, the dict stack, the journal and the
loop frames; round 3 found the resource categories were missing, which showed
that listing roots cannot be complete (gstates, fonts, ...). So the guard
moved to where the recursion is: `impl Drop for Dict` frees a dictionary's
nested dicts and arrays with a worklist, whichever field of the `Interp`
(or whatever mid-run `undef`) releases the last reference. It sits on `Dict`,
not `Object`, so the "no `Drop for Object`" rule holds, and it also fixes the
mid-run case that #631 tracked. Tests:
`dropping_an_interp_with_deeply_nested_dicts_does_not_overflow_the_stack`
(100000-level dict, array and alternating chains in userdict, on the operand
stack, open under `begin`, registered with `defineresource`, and held only by
the journal) and `freeing_a_deeply_nested_dict_chain_mid_run_...` (`undef`);
both abort with the `Dict` guard disabled. Rounds 4 and 5: `try_borrow_mut`
in `Drop for Interp` (a caller holding a borrow of a popped userdict must not
panic the drop), and gstates in the worklist, since a font dict can hold a
gstate that holds a font dict (`..._through_gstates_...`). Round 6 added the
colour-space operand objects of a gstate (`Separation`'s colorant is not
type-checked, so a dict can sit there). Still on drop glue: the operands
of a Show/Image frame.

**Cost of the `Dict` guard.** Freeing a dict now moves its nested containers
to a worklist instead of letting drop glue walk them. A microbenchmark that
creates and frees 400k small dicts (some holding an array and a dict) went from
0.17 s to 0.19 s CPU; `gallery/fern.ps` is unchanged. Not a goal, but noted.
