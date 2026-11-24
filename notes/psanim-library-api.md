## Host integration API for embedding applications (branch feature/psanim-library-api, 2026-11-23)

Built on `feature/psanim-library-api`, branched from `feature/0.7.1` at
`72a2c4e`, as the PSCat half of psanim (an animation engine in its own
repository that uses pscat as a library, never the CLI). No issue number yet:
the branch is offered for review as one change, and this fragment is its
write-up. Rename the file to `<issue>-psanim-library-api.md` when it has one.

### What was built

A new supported module, `pscat::host`, and a few methods beside `Page`:

| Need of an embedding application | API |
|---|---|
| Give programs operators with host state | `Interp::register_operator(name, closure)` |
| Exchange structured data | `host::Value`; `pop_value`/`push_value`, `lookup`/`define_value`, typed `pop_*`/`push_*`, `operand_count`, `clear_operands` |
| Work on the path a program built | `host::PathSegment`; `Interp::current_path`, `set_current_path` |
| Read frames without copying | `Interp::canvas_size`, `canvas_premultiplied_rgba8`; `Page::premultiplied_rgba8` |
| Layers, kept canvases, host compositing | `Interp::swap_canvas`; `Page::transparent`, `from_premultiplied_rgba8`, `premultiplied_rgba8_mut` |
| Report errors as data, located in the program | `Interp::error_details` -> `host::ErrorDetails` with `backtrace: Vec<TraceFrame>` |
| Stop a run from another thread | `Interp::set_interrupt`, `PsError::Interrupted` (PLRM name `interrupt`) |

Tests: `tests/host_api.rs` (26 tests) and the doc tests in `src/host.rs`,
`src/page.rs` and the crate root. `docs/guides/library.md` has the user-facing
section.

### Decisions

- **An operator stays a function pointer.** `Operator { name, func: fn }` is
  what the step loop calls, and making it a closure would put an indirection
  on every operator call. Host operators are instead 256 monomorphized
  trampolines (`trampoline::<N>`), each of which finds closure `N` in the
  interpreter. The cost is a fixed limit (`MAX_HOST_OPERATORS`), which no
  consumer has come near (psanim registers about 70).
- **`Value` copies composites.** A host gets plain data, not `Rc` handles into
  VM, so it cannot hold a borrow across a run or see a `restore` change an
  array under it. An object with no plain form (procedure, save, file) is
  `Value::Other`, which can be pushed back: enough to hold a `save` object
  outside the VM, which the frame loop needs.
- **Paths cross in user space.** The interpreter stores device-space `f32`
  points; `current_path` maps them back through the inverse CTM and says so
  (coordinates return within a fraction of a device pixel, not bit for bit).
- **The canvas is exchanged, not lent mutably.** `swap_canvas` moves a `Page`
  in and the old one out, so tiny-skia stays private and the 16-bit shadow
  (`deep`) is dropped at one well-defined point. A host that wants to filter
  the canvas in place swaps it out, edits the `Page`, swaps it back.
- **The backtrace costs nothing until an error.** Names are found by identity
  search of the dictionary stack when an uncaught error leaves the machine.
  The one hot-path change: a procedure frame popped as it issues its last
  element is moved into a one-slot record instead of dropped (same work), with
  the depth and the parent's `pc` to tell later whether the record is still
  current. `benches/perf`: fib 27 88-90 ms before, 89-90 ms after.
- **`Interrupted` is raised outside the step loop**, between slices of
  `run_source`, so the loop itself is untouched. A host that drives `step_n`
  itself reads its own flag.

### Compatibility

Additive. No existing signature or behaviour changes; `PsError` gains a
variant (the enum is `#[non_exhaustive]`). The full suite passes unchanged:
3688 tests, 0 failures (`cargo test --profile ci`, 2026-11-23).

### Not done, and why

- **Virtual files** (a host serving `(name) run` from memory). psanim loads
  its prelude with `run_str`, which was enough.
- **Drawing a raster under the CTM from Rust** (a host `image`). psanim
  composites layers itself at canvas size; a program can still use `image`.
- **`Send` interpreters or VM snapshots across threads.** Each psanim worker
  loads its own interpreter (about 2 ms), which made this unnecessary.
- **A thread-local teardown hazard, observed, not fixed:** dropping an
  `Interp` from a thread-local destructor panics, because the name interner's
  own thread-local can be destroyed first. Hosts should drop interpreters
  before their thread exits.
