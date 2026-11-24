# #1404: dropping an `Interp` from a thread-local destructor

The issue guessed the name interner. It is not: the backtrace of the abort is
`Drop for Interp` -> `Drop for Dict` -> `PsArray::teardown_last` ->
`EMPTY_STORE.with(..)` in `src/object.rs`. `EMPTY_STORE` is the placeholder
`Rc` that the iterative array/dict teardown swaps in. It is created on first
use (the first array freed on the thread), so it is registered for
destruction at that moment, and a thread-local holding an `Interp` registered
before it is destroyed after it. `with` on a destroyed slot panics, inside a
destructor, which aborts the process ("thread caused non-unwinding panic").

The interner (`src/name.rs`) is not touched by any `Drop` on this path
(dictionaries are keyed by id; `resolve` runs only to render an error), and
the other thread-locals (`guard`, `memtrack`, `mesh`, `shading`, `handwriting`,
`wasm`) are either `const` with no destructor, already `try_with`, or used
only inside a call, not from a `Drop`.

Fix: `object::empty_store()` uses `try_with` and falls back to a fresh empty
store. No change to `name.rs` or `interp.rs`.

Reproduction (the issue's sketch does not abort as written; it needs an
array freed on the thread so `EMPTY_STORE` exists, and two thread-locals so
that one is destroyed after it): `tests/interp_thread_local_drop.rs`, each
case in a child process. Before the fix `one` and `two` fail (child killed by
the abort); after, all five pass. fib 27 before/after: 91 ms / 85-88 ms
(noise).
