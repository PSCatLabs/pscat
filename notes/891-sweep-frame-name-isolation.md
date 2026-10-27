# #891: sweep frames render exactly as standalone runs

Follow-up to #868 (`notes/868-band-preamble-name-ids.md`). Name ids are
handed out per thread in interning order, and `forall` over a dict's name
keys follows the ids. Two things made a sweep frame differ from a standalone
render of the same seed or value:

1. **The `--sweep NAME=` preamble.** `run_sweep` ran `/NAME v def` before the
   program, so NAME took an id before every name the program interns ahead of
   its own first mention of NAME, shifting all of them by one.
2. **One interner for every frame.** Frames ran one after another on the main
   thread. Names an earlier frame made (`cvn` keys that depend on the seed or
   the value) were already taken when the next frame ran, so that frame's new
   names got other ids. `--sweep-seed` (no preamble) was hit by this one.

`pscat sheet --sweep-seed`/`--sweep` had both, and one more: its probe
interpreters and caption text render on the main thread before any cell, so
even the first cell started from a used interner. A sheet of several files
also leaked one file's names into the next.

## Fix

- `main.rs` `run_on_fresh_thread(f)`: runs `f` on a new scoped thread
  (8 MiB stack, the macOS main thread's, as `BAND_STACK`), calls
  `memtrack::fold_at_thread_exit()` first, re-raises a panic. `run_sweep`
  runs everything that touches the frame's `Interp` in it (construction,
  limits, preamble, run, error report, flush, last-page clone, halftone) and
  hands back the page and three flags; status lines, PNG writes and the
  contact-sheet blit stay on the main thread, so output order is unchanged.
  Frames still run one at a time, one frame in memory.
- The preamble runs inside `pscat::name::detached`, as the band preamble does.
- `subcmd/sheet.rs` `render_cell` does both, through the same helper.
- MCP `review_sheet` is unaffected: every cell is its own `pscat` child
  process (`mcp_server.rs`, `Command::new(pscat_bin()?)`), and it has no
  sweep axis.

### Why not reset the interner between frames

A reset is only safe if no `PsName` from the last frame outlives it. Nothing
proves that (thread-locals such as caches, the probe interpreter, a name held
by the caller), and a stale name would then alias a new one with the same id.
A fresh thread is the mechanism #844 already proved for bands, and it resets
every other thread-local too.

## Tests

`tests/sweep_identity.rs`: a program fills dict `d` with 40 seed-dependent
`cvn` keys before it first mentions `/Shade`, then dict `e` with 40
`/Shade`-dependent keys, and paints both in `forall` order. Each frame of
`--sweep-seed 1,2,3` and `--sweep Shade=1,2,3`, and each `pscat sheet` cell
for both, must equal a standalone render (PNG bytes; decoded pixels for
sheet cells). On the old code: `--sweep` frame 1 (preamble only) and
`--sweep-seed` frame 2 (shared interner only) fail, and so does sheet cell 1.
With only the `detached` change, the sweep frames 2+ still fail, so the test
covers each cause on its own. A single dict whose keys follow the `/Shade`
mention kept its order under the preamble's one-id shift, which is why the
program has two dicts.

## What "standalone" means, and what is still not identical

pscat has no `-D` flag, so the standalone run for `--sweep NAME=v` is the
program with its own default for NAME set to `v` (`/NAME where { pop } {
/NAME v def } ifelse`), which is what the tests compare against.

The swept NAME keeps its detached id (`DETACHED_BASE + k`); the sequential
slot it reserves when the program first mentions it is a placeholder. So a
program that puts NAME itself into a dict as a key and `forall`s it can still
come out in another order: a probe with `d /Shade 40 put` plus 40 `cvn` keys
differs between a standalone run and `--sweep Shade=1`. `forall` over
`userdict` differs anyway (the sweep's userdict holds NAME). This is the
residual #868 recorded for the band names.

## Cost

A guard number, not a goal. `--sweep-seed 1:64 --contact-sheet` of a
one-rectfill 20x20 pt page, release binaries, minimum of 9 runs, per-frame
cost = (64-frame time - 1-frame time) / 63: base 35 us, head 84 us, so about
50 us per frame for the thread and a fresh interner (systemdict's names are
interned again in each frame, as in a standalone run). Any real frame takes
milliseconds.
