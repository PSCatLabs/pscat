# #895: spool jobs on fresh threads; the detached-name residual; bands tidy

Follow-up to #868 (`notes/868-band-preamble-name-ids.md`) and #891
(`notes/891-sweep-frame-name-isolation.md`).

## 1. Each `--spool` job runs on a thread of its own

`--spool` ran every job's interpreter on the event loop's thread, one after
another. Name ids are per thread and handed out in interning order, and
`forall` over a dict's name keys follows the ids, so names an earlier job
made could change a later job's `forall` order: a job could draw
differently from a standalone run of the same file.

The issue said to use `run_on_fresh_thread` (`main.rs`). That doesn't fit
the spool, for three reasons:

- The spool doesn't run a job to the end in one call. `window.rs` steps it
  `steps_per_frame` at a time from the winit event loop, which has to stay
  on the main thread on macOS, so the job draws live and a `--max-time`
  limit counts the window's pace (#701).
- `run_on_fresh_thread` runs its closure to the end and joins. Wrapping a
  whole job in it would lose the live drawing and that pacing.
- It lives in the binary crate, and the spool loop is in the library
  (`window::run_spool_with_limits`).

So `window.rs` has `JobThread`: a job's own thread, with an 8 MiB stack (the
main thread's, as `FRESH_THREAD_STACK`). It calls
`memtrack::fold_at_thread_exit()`, builds the `Interp` there (`Interp` is
`!Send`), sets the limits and queues the source. The event loop sends it
boxed closures over a channel, one at a time, and waits for each reply.
Each frame is one "step `steps_per_frame`" call (it returns the error report
text on an error) and one "draw the view" call. The draw call fills a
window-sized `Vec<u32>` that is reused between frames, and the loop copies
it into the softbuffer. The arrow keys ask the job for its page count.
Dropping the handle closes the channel and joins, which frees the
interpreter. `poll_spool` drops the previous job before it starts the next,
so a memory limit still counts only what the new job grows. The last job
stays alive after it ends, so its pages can still be browsed. The window and
`--interactive` modes keep their interpreter on the event loop's thread,
unchanged.

A panic on the job's thread was already reported by the panic hook. The next
call returns `None`, and the spool reports the job as crashed and moves on.
Before, a panic took down the whole window.

### Cost

Measured with a throwaway `#[ignore]` test in a release build, minimum of
9 runs:

- **Per job:** a fresh thread (`JobThread::start` plus join) against a fresh
  `Interp` on the same thread, for a `1 pop` job at 612x792. 184 against
  124 us, so about **60 us per job**, close to #893's 50 us per frame. Most
  of it is systemdict's names being interned again in an empty interner, as
  in a standalone run.
- **Per frame, stepping:** 100 steps took 0.9 us locally and 3.5 us through
  the channel, so about **3 us of round trip**.
- **Per frame, drawing:** 1.18 ms locally and 1.32 ms remotely for a
  1224x1584 window, and 4.7 ms against 5.4 ms for 2448x3168. The extra
  0.15 to 0.7 ms is one copy of the frame into the buffer. Before the frame
  `Vec` was reused, a fresh zeroed `Vec` per frame cost 0.5 to 2 ms.

Against a 16 ms frame, that's small.

### Test

`window.rs` `a_spool_job_renders_as_a_standalone_run` drives the real path.
It builds an `App` with no window, calls `poll_spool` and `tick`, and runs two
jobs:

- job 1 interns `/39` down to `/0` with `cvn`;
- job 2 puts `/0` to `/39` into a dict and paints a stripe per entry in
  `forall` order.

Job 1 permutes job 2's ids rather than shifting them, because #868 found that
a uniform shift can keep the order. Job 2's canvas must equal, byte for
byte, a standalone run on a spawned thread. On the old code it differs.

## 2. Detached names keep their detached id: not changed

A band name or `--sweep` NAME keeps its detached id (`DETACHED_BASE + k`).
So a dict the program fills with that name as a key, and then `forall`s, can
come out in a different order from a plain run. Re-run on this branch: the
#891 probe (`d /Shade 40 put` plus 40 `cvn` keys, painted in `forall` order)
still differs between a standalone render and `--sweep Shade=1`. `--bands 2`
of the same file matches the plain render, because the band names are not in
the dict.

Giving the name its sequential id at its first mention isn't clean:

- **Re-keying.** The name's hash bucket was fixed when the preamble stored
  it, from the detached id. Every dict holding it (userdict at least, and
  any dict the preamble's values reached) would need re-keying, and nothing
  records which dicts those are. Every `PsName` value already made, on the
  stacks or in arrays, would also have to change id.
- **Indirection.** Mapping detached ids to their eventual slot inside
  `IdHasher` doesn't work either. The slot isn't known when the key is
  first hashed, and a mapping consulted on every probe puts a table lookup on
  the hottest path (#868 kept that path to one compare).
- **userdict differs anyway.** A banded or swept `userdict` holds extra
  entries, so `forall` over it can't match a plain run whatever the ids.

So the documented caveat stays. It's now in `name::detached`'s doc comment
too.

## 3. Tidy

`main.rs` splits the helper. `join_on_fresh_thread(name, f)` returns an
`io::Result<thread::Result<T>>`. `run_on_fresh_thread`, for sweeps and
sheets, is that helper plus `resume_unwind`. `bands.rs` calls
`join_on_fresh_thread` directly, so a band's panic is still that band's
failure ("band i/n crashed") and doesn't take down the process. The band
thread keeps its name (`band-N`). `BAND_STACK` and `FRAME_STACK` are now one
constant, `FRESH_THREAD_STACK`.

## Verification

`scripts/identity_check.sh --base-bin/--head-bin` with `--loader
lib/artkit.ps`, plain and with `--pscat-args "--bands 2"`. The numbers are
in the PR. Plain and banded headless renders take no new code path; the
bands change only moves the thread spawn into the shared helper. So nothing
can differ, and there is no negative control. The spool path has no
headless CLI output, so `a_spool_job_renders_as_a_standalone_run` is its
check.
