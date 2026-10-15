# #505: `grestore` with nothing to pop, and the colour readbacks (2026-10-14)

Both follow up the "Not done" bullets in `notes/494-clipsave-cliprestore.md`.
Ghostscript 10.07.1 is the reference; every behaviour below was probed in
gs first (`-dNODISPLAY`, `==`), then matched.

## `grestore` and `grestoreall`

| program | gs | pscat before |
|---|---|---|
| `5 setlinewidth grestore currentlinewidth ==` | `1.0` | `5.0` |
| `2 setlinewidth gsave 3 setlinewidth grestore grestore W` | `1.0` | `2.0` |
| `1 lw gsave 2 lw save 3 lw grestore W grestore W restore W` | `2.0 2.0 2.0` | `1.0 1.0 2.0` |
| `7 setlinewidth grestoreall W` | `1.0` | `7.0` |
| `2 lw gsave 3 lw gsave 4 lw grestoreall W` | `1.0` | `2.0` |

(`lw` is `setlinewidth`, `W` is `currentlinewidth ==`.)

So, as in gs:

- **At a `save` boundary** (no `gsave` open since the innermost `save`),
  `grestore` brings back the state the `save` took and pops nothing. A
  second `grestore` does the same again. pscat used to pop the `gsave`
  from *before* the `save`.
- **At the bottom** (nothing saved, no `save` live), `grestore` brings back
  the job's initial state: the CTM, colour, line parameters, clip, path,
  and no font. It used to be a no-op. `grestoreall` with no `save` does the
  same. It used to stop at the oldest `gsave`'s copy, but gs goes past that
  to the initial state.
- In both cases the level starts with an **empty `clipsave` stack**, because
  gs's `grestore` there ends in a fresh `gsave`. The `save`'s snapshot still
  holds the entries made before the `save` (`do_save` clears the stack only
  after it snapshots). `Gfx::grestore_at` clears them, so `0 0 100 100
  rectclip clipsave 10 10 50 50 rectclip save grestore cliprestore` leaves
  the clip at save time (10..60), as gs does, not 0..100.

The initial state is rebuilt with `Gfx::initial_state`, the constructor's
own state, not snapshotted. Nothing changes the graphics state between
`Gfx::with_scale` and the job's first token: the front ends only read
pages back. `with_scale`, `init_graphics` and the bottom `grestore` share
that one function.

pscat's own paired `gsave`/`grestore` calls (`rectfill`, `rectstroke`, a
`shfill`, ...) still use the plain `Gfx::grestore`, which only pops. Only
the operator goes through `Interp::do_grestore`.

## `currentgray` and the other readbacks

The `0.49999999999999994` comes from the **conversion's float order, in
`f64`**: `0.3 * 0.5 + 0.59 * 0.5 + 0.11 * 0.5`. A second drift has the same
root. The colour is held in `f32`, so `0.3 setgray currentgray` read back
the widened `0.30000001192092896`. gs's reals are `f32`, so its sum rounds
to `0.5` and `0.3` stays `0.3`.

The fix is in the operators, not in real printing.
`currentgray` and `currenthsbcolor` round the value to `f32` and push
the `f64` nearest that `f32`'s shortest decimal (`color_readback` in
`src/ops/graphics.rs`). A read-then-set round trip lands on the same
`f32` as before, so copying a colour changes nothing (tested). No
library calls either operator (`lib/paintkit.ps` mentions `currentgray`
only in a `%%SelfTest` block, with a 0.001 tolerance). drykit's `grestore` shadow pops its own
`/pcs` stack and marks the batch `/dirty` on a pop it never saw pushed, so
a `grestore` at a boundary is followed conservatively.

**`currentrgbcolor` is deliberately not rounded.** The first version of
this change rounded it too, and gs prints `0.3 0.6 0.9 setrgbcolor
currentrgbcolor` as `0.3 0.6 0.9` where pscat prints
`0.30000001192092896 0.6000000238418579 0.8999999761581421`. But
`lib/paintkit.ps` does arithmetic on `[ currentrgbcolor ]` (the brush
base colours), and the rounding moved the committed
`gallery/renders/alpine_lake.png` still by one LSB in one channel at one
pixel, (759, 1016) at 2x. That is invisible, but the still is committed,
so the change is left out here. Whether to accept a regenerated still is
filed as a follow-up.

### Still different

- **9-digit values.** gs prints a real with up to 9 significant digits,
  and pscat prints its shortest round-trip form. For the same `f32`, gs
  prints `0.362000018` where pscat prints `0.36200002`
  (`0.2 0.4 0.6 setrgbcolor currentgray`). The test compares those lines as
  `f32`s. Making pscat's reals print like gs's would change every `==`,
  `cvs` and text label of a real, so it is out of scope.
- **Values that come out one ulp off.** `0.3 0.6 0.9 setrgbcolor
  currentgray` is `0.543000042` in gs (an `f32` one ulp above the one
  nearest 0.543) and `0.543` in pscat. `0.3 0.6 0.9 sethsbcolor
  currenthsbcolor` gives saturation `0.6` in gs and `0.59999996` in pscat:
  pscat's rgb to hsb works on the widened `f32`s in `f64`. Neither case is
  pinned.
- **No font at job start.** gs starts a job with a font, so `show` works
  after a bottom `grestore`. pscat starts with none, so the same `show` is
  an `invalidfont`, as it is in a fresh pscat job. That was true at job
  start before this change; a bottom `grestore` now reaches the same state.
- **Type 3 `BuildChar`** (#500): a glyph procedure's unmatched `grestore`
  now resets to the job's initial state rather than to the state at
  `show`. Nobody does this.
- `currentcmykcolor`, `currentcolor`, `currentlinecap`, `currentlinejoin`,
  `currentmiterlimit` and `currentstrokeadjust` are not defined in pscat.
  Filed as a follow-up.

## Evidence

- `tests/grestore_boundary.rs`: 16 programs pinned to gs output, plus a
  local-only gs re-run (`ghostscript_prints_the_pinned_lines`). Also a
  check that a bottom `grestore` gives the same readbacks as job start at
  scale 1 and 2, and the read-then-set round trip.
- `tests/clipsave.rs` prints `currentgray` directly (`0.5`) instead of
  rounding it.
- Three older tests pinned the old behaviour and now pin gs's:
  - `tests/vm.rs` `grestoreall_without_save_pops_to_bottom` expects `1.0`,
    not `3.0`. It already said gs prints `1.0`. `docs/dev/VM.md` no longer
    lists `grestore` at a save boundary as a deviation.
  - `tests/drykit.rs`'s grestoreall case sets its colour again after the
    `grestoreall`, as its initgraphics case does.
  - `tests/headline.rs` `a_draw_proc_that_leaks_a_gsave_and_fails_is_fully_unwound`
    used a bottom `grestore` being a no-op as its probe. It now puts a
    sentinel `gsave` under the call.
- Identity: see the PR (`scripts/identity_check.sh --base-bin/--head-bin`
  over every tracked `.ps`/`.eps`).
