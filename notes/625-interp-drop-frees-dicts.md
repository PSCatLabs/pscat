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

**Known limit.** Freeing a very deeply nested dict chain recurses once per
level (only `PsArray` drops iteratively); about 10000 levels overflow a 2 MB
test thread. It already applied to `--sweep`, `--spool`, selftest and any
`undef`, and is newly reachable for library users who used to leak such a
graph. Follow-up: #631.
