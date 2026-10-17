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

**Deep nesting (Codex review, round 1).** A first version let the compiler's
drop glue free the contents, which recurses once per nesting level of dict
(only `PsArray` drops iteratively): about 10000 levels overflow a 2 MB thread,
and for library, MCP and wasm callers that is a new input-triggered abort where
dropping used to leak. `Drop for Interp` now feeds a worklist, `object::teardown_iteratively`, that
unwraps uniquely held dicts and arrays (so dict/array mixes are covered too).
Round 2 widened the roots from systemdict and userdict to everything that can
keep a user-built chain alive: the operand stack, extra dicts left on the dict
stack by `begin` (the chain's last reference is otherwise released by field
glue), the save journal, and the procedure and `forall` source of running
loops. Left to drop glue: graphics-state snapshots and Show/Image frames
(font and image data). `dropping_an_interp_with_deeply_nested_dicts_does_not_overflow_the_stack`
builds 100000-level chains (dicts, arrays, alternating; in userdict, on the
operand stack, open under `begin`, and held only by the journal) on a test
thread; it aborts with either the dstack or journal handling removed. Not covered, and
tracked in #631: a deep chain freed *during* a run (`undef`, `restore`,
overwriting a key), and one held only by the save journal or execution stack.
