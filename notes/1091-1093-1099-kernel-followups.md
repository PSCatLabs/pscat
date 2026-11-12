## Kernel follow-ups from the fractal_clouds rerun (2026-11-10)

fractal_clouds reran fc#117 on test build b959248 (run 42724164396). All
8 scenes were byte-identical and 6 of 8 compiled. pnw_coast and
tropical_island still ran in the interpreter, and their report raised
three pscat findings (#1091 and #1093 comments). This entry covers the
fixes, plus #1099 and a second compile-time pass.

### #1091: a size cap for the shared pass

The cap counts code **before** dead-code removal. Shared bodies also
compile without their callers' constants. So a sharing compile counts 3–4×
the kernel it keeps: pnw_coast counted 874k for a 251k kernel.

Under the single 400k cap, `/auto`'s shared retry hit the cap three times,
all deep in water reflection. The inlined pass hit it once, on the main
path (`fc-haze`). `choose` keeps the pass with fewer soft branches, so it
kept the inlined one, and 11,711 pixels fell back.

That was the heuristic working as written, not stale state, as first
guessed on #1091.

- Sharing compiles (`/large`, `/all`, `/auto`'s retry) now have
  `MAX_INSNS_SHARED` = 2M.
- `/never` and `/auto`'s first pass keep 400k. That pass only asks "does it
  fit inlined?", and a larger cap there only makes it fail later. Raising
  the one global cap to 2M took tropical_island's compile from 2.2 s to
  5.9 s.
- `MAX_SUB_MADE` goes from 1.6M to 4M. tropical_island makes about 900k.
- **`.kernelcaps`** (`inline shared made .kernelcaps`, 0 = default) is a
  test hook. It lets the cap tests stay small: the made-cap test went from
  979 MB peak RSS to 244 MB. It also brings back the test of `choose`'s
  "shared retry refused" arm.

**Peak RSS** at 160×100 is now 324–439 MB on the three largest scenes.

### #1093: what the purity scan allows

On unpatched pnw_coast the "cannot run alone" list had 16 reasons. About
half came from one dead branch: `FcClCompiled not { fc-clouds-compile } if`
with `FcClCompiled` true.

A first version allowed four forms read from the text:
- stack `n copy`;
- `n array astore`;
- `/x load`;
- a branch that a constant boolean guard never takes.

The independent review found it unsound, with repros that differ from
`/interpret`:
- a pixel can rebind `not` or `array`;
- it can rebind the guard with a string key, `(Done) true def`;
- it can rebind it with a name fetched from a procedure,
  `{Done} 0 get true def`;
- it can fetch `copy` or `astore` out of a procedure with `get` and run it
  on anything (why the original scan refused `copy`).

A second version made `load` pure by scanning the binding of every name
in reach. Re-review found that unsound too: `load` also takes a string or
a number as its key. `(put) load` returns the `put` operator, and a
substring made with `getinterval` can name anything.

What stayed is only the reason list: `PSCAT_KERNEL_TIMING=1` lists every
reason instead of "and N more". The scan is otherwise as it was. All the
reviewers' repros are a test
(`forms_that_cannot_be_told_from_the_text_interpret_the_whole_image`).

fractal_clouds fixed the uncompiled branches themselves (#142 `FsMixWt`,
#143 the trunk `aload`). With those, no scene has a soft branch, so the
scan no longer runs on them.

### #1099: SIMD for the fallible ops

The executor sent `div`, `sqrt` and `lt le gt ge` through a per-lane call
into `lane_bin`/`lane_un`, one per lane. `neg`, `abs` and `floor` read
each lane through `OpRef::at`'s match. None of them vectorized. They now
run branch-free over whole lane arrays, with a compare building the
failure mask.

- **Division** must be computed unconditionally, then selected: `if z {0}
  else {x / y}` stays scalar.
- **Release aarch64** now has `fdiv.2d`, `fsqrt.2d`, `fcmgt`/`fcmge.2d`
  and `frintm`/`frintp`/`frintz.2d` in `Machine::block`, and no scalar
  `fdiv`.
- **Speed:** mandel, relief and spheres run 10–15% faster on one thread.
  The fractal_clouds frames barely move (next section).

### Compile time, second pass (#1094 follow-up)

Profiled with `kernelcompile` alone on golden_coast, joins took about 60%
of compile time. Three changes:

- `par_moves`' hazard check was quadratic; it is now a set of
  destinations.
- `join2` skips the names of a kernel scope whose names are still shared
  (copy-on-write) by both branches.
- `Frame::ids` and `wnames` use the name-id hasher instead of SipHash.

Compile time: golden_coast 1.83 → 1.28 s, pnw_coast 1.67 → 1.19 s. The
kernels are unchanged: the same instructions, registers and lane-step
counts.

### Results (build1 = 747ccee, fractal_clouds ea18d58, 160×100, M4 loaded)

All 8 scenes compile with nothing retired, and each is `cmp`-identical to
a fresh `/interpret` render: 23–79× faster.

| scene | compiled | interpreted | speedup |
|---|---|---|---|
| tropical_island | 2.4 s | 75 s | 32× |
| pnw_coast | 2.0 s | 53 s | 26× |
| golden_coast | 1.3 s | 30 s | 23× |

### What did not pay

- **Lane width.** 8 and 32 lanes were both slower than 16 on golden_coast,
  pnw_coast and mountains (32 by 1.4–2×). These kernels branch a lot.
- **GPU preview.**
  - **mountains** (no subroutines) passes fractal_clouds' SAME-LOOK test:
    PSNR 55.5 dB, mean delta 0.013. Its dispatch takes 20 ms, against
    about 200 ms on the CPU. But every process spends 5–12 s building the
    shader, almost all of it in naga's WGSL front end (`Typifier::grow`).
    fractal_clouds renders one frame per process.
  - **golden_coast and the other shared-subroutine scenes** exceed the
    GPU's 1M-instruction inlining limit.
  - **Subroutines as WGSL functions** sharing `var<private>` registers was
    tried and reverted: Metal's compiler ran for over 10 minutes on
    golden_coast.
  - A real fix would build naga IR directly (skipping the WGSL front end)
    and keep calls out of line in MSL. Neither is in reach through wgpu
    today.
- **Approximate math (#1100) for these scenes.** At 640×400, golden_coast's
  run is almost all in `Machine::block` itself, with `pow`, `log`, `sin`
  and `cos` under 4%. Vector transcendentals would not move these scenes.
  The cost is the executor walking about 170k instructions with about 4.4k
  registers per 16-pixel batch.

### Open

- **Executor run time on large kernels.** Candidates:
  - a compact instruction encoding (`Ins` is up to 128 bytes);
  - registers shared across subroutines that are never live together;
  - 2D pixel tiles, for coherence.
- Lowering to naga IR, if the GPU is to be worth it for one frame per
  process.
