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
`show`, and an open `systemdict begin`, keeps `Weak` handles to eight dicts,
drops the `Interp` and asserts every `upgrade()` is `None`, twenty times.
With the `Drop` body disabled it fails on systemdict, so it discriminates.

**Rendering.** Teardown runs after all output is written, so nothing
rendered can change; there is no control that renders differently, so the
two-binary identity check has nothing to prove here. The golden tests and
selftest pass unchanged.

**Exit cost.** See the PR for the timing of the CLI, which now frees the
dictionary graph at exit instead of leaving it to the OS.
