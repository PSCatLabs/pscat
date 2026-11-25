## Host integration API for embedding applications (branch feature/psanim-library-api, 2026-11-23)

Built on `feature/psanim-library-api`, branched from `feature/0.7.1` at
`72a2c4e`, as the PSCat half of psanim (an animation engine in its own
repository that uses pscat as a library, never the CLI). Issue #1403.
The branch is offered for review as one change, and this fragment is its
write-up.

### What was built

A new supported module, `pscat::host`, and a few methods beside `Page`:

| Need of an embedding application | API |
|---|---|
| Give programs operators with host state | `Interp::register_operator(name, closure)` |
| Let a long operator honour the limits and the flag | `Interp::check_limits(steps)` |
| Exchange structured data | `host::Value`; `pop_value`/`push_value`, `lookup`/`define_value`, typed `pop_*`/`push_*`, `operand_count`, `clear_operands` |
| Work on the path a program built | `host::PathSegment`; `Interp::current_path`, `set_current_path` |
| Read frames without copying | `Interp::canvas_size`, `canvas_premultiplied_rgba8`; `Page::premultiplied_rgba8` |
| Layers, kept canvases, host compositing | `Interp::swap_canvas`; `Page::transparent`, `from_premultiplied_rgba8`, `premultiplied_rgba8_mut` |
| Report errors as data, located in the program | `Interp::error_details` -> `host::ErrorDetails` with `backtrace: Vec<TraceFrame>` |
| Stop a run from another thread | `Interp::set_interrupt`, `PsError::Interrupted` (PLRM name `interrupt`) |

`push_value` and `define_value` return `Result<(), PsError>` (they refuse a
foreign opaque value, below). `host::MAX_VALUE_NODES` bounds one copy.

Tests: `tests/host_api.rs` (38 tests), `tests/host_api_memory.rs` (the copy
under `--max-memory`, its own binary with the tracking allocator) and the doc
tests in `src/host.rs`, `src/page.rs` and the crate root. `docs/guides/library.md` has the user-facing
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
- **A copy is bounded and charged.** `MAX_VALUE_DEPTH` alone let an array that
  holds itself twice copy out to 2^32 nodes. `MAX_VALUE_NODES` (2^20: one per
  element or entry, one per 32 string bytes, counted before the members are
  copied) caps it; past it `pop_value` is `limitcheck` and leaves the operand,
  and `lookup` gives the value back whole as `Value::Other`. Every 1024 nodes
  the copy looks at the running program's limits and the flag, and charges a
  step per 1024 nodes. Between runs only the memory cap applies: the clock and
  the step count belong to the run that ended.
- **An opaque value belongs to its interpreter.** Host operators are slots
  looked up in whichever interpreter runs them, and a save object is one of an
  interpreter's save levels, so `Value::Other` records the interpreter it came
  from (a process-unique id) and `push_value`/`define_value` refuse one from
  another with `invalidaccess`, anywhere in the value. Interpreters on separate
  threads never meet this: an `Opaque` holds an `Rc` and stays on its thread.
- **Paths cross in user space.** The interpreter stores device-space `f32`
  points; `current_path` maps them back through the inverse CTM and says so
  (coordinates return within a fraction of a device pixel, not bit for bit).
- **The canvas is exchanged, not lent mutably.** `swap_canvas` moves a `Page`
  in and the old one out, so tiny-skia stays private and the 16-bit shadow
  (`deep`) is dropped at one well-defined point. A host that wants to filter
  the canvas in place swaps it out, edits the `Page`, swaps it back.
- **The backtrace costs nothing until an error, and little then.** Names are
  found by identity search of the dictionary stack when an uncaught error
  leaves the machine: one pass over at most 65,536 name entries for all the
  frames together, copying nothing (it runs after the limits stop looking,
  for every uncaught error, CLI programs included, so it must be bounded by
  constants). A procedure under a string key, or past the scan, is unnamed.
  Under a scanner the tail record also keeps the scanner's token offset, so a
  procedure that returned is not blamed for a later top-level error.
  The one hot-path change: a procedure frame popped as it issues its last
  element is moved into a one-slot record instead of dropped (same work), with
  the depth and the parent's `pc` to tell later whether the record is still
  current. `benches/perf`: fib 27 88-90 ms before, 89-90 ms after.
- **`Interrupted` is raised outside the step loop**, before every slice of
  `run_source` (the first included) and once after the last, so the loop
  itself is untouched and a short run with the flag raised fails too. It is
  uncatchable however it arises: `PsError::is_uncatchable` (a limit error or
  `Interrupted`) is what `recover` tests. A host operator sees the flag through
  `check_limits`. A host that drives `step_n` itself reads its own flag.
- **`clear_operands` inside an operator pops through the operand ledger**, so
  a failure after it puts the caller's operands back; outside an operator it is
  still a plain clear.

### Review round 1 (Codex, PR #1410)

Eight findings against `f6ed4e7`, each confirmed by a test that failed (or, for
the copy, hung past a 60 s alarm) on that head before its fix; finding 3's test
needs the new method, so it did not compile there. One commit per finding (4
and 5 together). Hot path, best of three, base
`f6ed4e7` against the fixed head, interleaved: `benches/perf` fib 27 88 ms
and 87 ms; the #863 dispatch groups (instructions retired per group, the empty
loop subtracted) `1 pop` 236/234, `1.5 2.5 mul pop` 507/503, `x y mul pop`
687/687, an empty procedure call 238/238, `{ 1 pop }` call 544/544,
`1 2 3 1 index pop pop pop pop` 1103/1096.

### Review round 2 (Codex, PR #1410)

Four more holes of the same class in the round-1 code, three fixed, each with a
test that failed on `deca3be` first:

- **A dictionary key went around the copy's bounds.** A key that is not a name
  or a string was spelled with the unbounded `repr()`, so an array key holding
  itself twice expanded to 2^32 elements (`d extract` hung past a 60 s alarm).
  It is now spelled within fixed bounds (depth 4, 16 elements, 256 bytes) and
  counted as 8 nodes; a name or string key, and a name value, counts by its
  length like a string value.
- **A backtrace copied a procedure's name whole.** Names are cut to 256 bytes
  at a character boundary, with `...`, without copying the rest.
- **`lookup` copies were not charged.** `lookup` takes `&self`, so its steps
  (a refused copy's work included) go into a `Cell` the next look at the limits
  adds to the step count (`sync_decode_budget`, which already gathers decode
  and mesh work); a copy between runs is free. `pop_value` likewise charges the
  work of a refused copy.
- **Not fixed: the name scan's cost follows a dictionary's capacity, not its
  entries.** A dictionary that grew and was emptied keeps its buckets, and
  `HashMap::iter` steps over the empty ones without spending the 65,536-entry
  budget. Measured: after 4,194,304 entries were defined and removed, an
  uncaught error 20 frames deep took 0.28 ms more than with an empty
  dictionary, against 4.6 s to build the dictionary. The cost is bounded by
  capacity the program already paid for (and the memory cap allowed), so it is
  left as a known limit.

### Transparency groups (merge with feature/0.7.1, #1406)

#1417 landed first, so the branch was merged with it (`sync_decode_budget`
charges both the host steps and the group work). While a group is open the
raster is its layer, so every host-side read or replacement of the canvas was
audited, and the host sees the page below the layer:

- `canvas_size`, `canvas_premultiplied_rgba8`: already `page_pixmap()`, the
  bottom page. Unchanged.
- `Interp::pages`, `page(i)`, `into_last_page`: copied the layer, so a host
  stepping a program between slices read the group's marks over a transparent
  page. Now the bottom page.
- `swap_canvas`: swapped the layer, handing the host a layer as "the canvas" and
  putting its page in the group's place; inside a pattern's `PaintProc` it
  swapped a tile for a page-sized raster (a `debug_assert` panic in the `ci`
  profile). Now it swaps the page below the layer, and is refused (the `Page`
  returned) inside a `PaintProc`.
- `set_transparent_background`: filled the layer, leaving the page opaque. Now it
  resets the page; the layer keeps its marks.
- Interrupt (flag or a host operator's `Interrupted`): composited the open groups
  in full, uncharged. Now it drops their layers at once, as a fired time limit
  does (`unwind_interrupted`). A step limit still composites (its marks stay).
- Charging: a swap is two pointer exchanges and a page reset a fill, as before;
  the interrupt path does less work than it did. Page copies by `pages`/`page` are
  host reads, as before. Tests: `tests/host_api_groups.rs`.

### Compatibility

Additive. No existing signature or behaviour changes; `PsError` gains a
variant (the enum is `#[non_exhaustive]`). The full suite passes unchanged:
3701 tests, 0 failures (`cargo test --profile ci`, 2026-11-24, after review
round 1). For programs that do not use the host API, round 1 changed nothing
visible: `scripts/identity_check.sh` with the two binaries rendered 127 of 127
pages of the 96-file artkit corpus identically (control: a marker built into
the base binary only), and the CLI's error reports are byte-identical.

Within this branch, round 1 changed the host API itself: `push_value` and
`define_value` return `Result`, a raised interrupt flag fails a run before it
executes anything (so a host must lower it before a recovery run), and
`pop_value` refuses a value past `MAX_VALUE_NODES`.

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
