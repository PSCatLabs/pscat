# Handoff: compute-kernels epic (2026-11-11, evening)

Session: https://claude.ai/code/session_016y8NtRr7b9NWRg1SctzFfp. It wrote
both the 2026-11-10 handoff and this update. The 2026-11-09 handoff was
session_016y8NtToNBqdyScG3LMGriu.

## Where things stand (end of 2026-11-11)

- **The epic.** `feature/compute-kernels` is at `6b7b5e5` and pushed. It is
  PR PSCatLabs/pscat#1061 (open, owner-merge, because it touches
  `Cargo.toml`). The ci, release and other workflows on `8df943d` were
  green, and the release dry run on `4953e28` is too.
- **#1101 (faster executor)** was merged at `46ad11b` and signed off by
  fractal_clouds (fc#117). See `notes/1101-faster-executor.md`.
- **#1102 (`kernelarray`/`kernelarrays`)** was merged at `08bf53a`. See
  `notes/1102-kernelarray.md`.
  - fractal_clouds' 512² erosion sample went from 4.35 s to 25 ms.
  - A fuzz that compares type and bits found a compiler bug the bytes had
    hidden: a Dyn integer zero could be -0.0. It is fixed with no added
    instruction (`IntZeroCheck`/`NegIntZero`).
- **#1100 (x86 tiers, `.kernelmath /fast`)** was merged at `8df943d`. See
  `notes/1100-simd-and-vector-math.md`.
  - The executor runs SSE2, v2 or v3 (AVX2, no FMA) copies. CI's x86 runners
    ran the v3 tier test, so the AVX2 code has executed on real hardware
    with the baseline's bytes.
  - `/fast` vectorises `sin`, `cos` and `atan`, bit-exact in the tests.
  - Still open on #1100, so it is `Refs`, not `Closes`:
    - AVX2 and AVX-512 timings (no x86 machine here; the note gives the
      commands);
    - AVX-512 needs Rust 1.89, above `rust-version` 1.88: the owner's call;
    - whether `/fast` becomes the default.
- **Codex review 3** (adversarial, `main...feature/compute-kernels`) found
  two exactness bugs, fixed in `99d666b` and merged at `4953e28`:
  - branch joins took 0.0 for -0.0, because `Op` compared immediates as
    floats;
  - x86 NaN payloads. `kernelarray` now stores every NaN as the quiet NaN.

  A Codex review of the fix diff was clean.
- **Test releases for fractal_clouds:**
  - **The first** was CI run 43108195586 (4953e28, manifest `a50dcd6`).
    fractal_clouds validated its correctness on every scene and size (fc#117
    comment 6809200666, pscat#1102 comment 6809204375). Its #145 bakes
    through `kernelarrays` are bit-identical to the loops, and the 512²
    sample went from 4.3–8.6 s to 21–76 ms.
  - **It found the release 5–10% slower per step.** The cause was #1100's
    fast-math opcodes in the block loop's dispatch. They are now in the
    side table (`5d383b4`), and cycles are back at build2's.
  - **The current one** is CI run 43122833835 (6b7b5e5, manifest
    `97b481d`):
    - aarch64 tar.gz SHA-256 `5662db76…47df`;
    - its binary's SHA-256 is `0aabf469…3f51`, in
      `kernels/ci-43122833835/x/`;
    - the local build is `kernels/build4-6b7b5e5/pscat` (`18aecec9…b3fc`).

    It was sent for fractal_clouds' timing rerun, which is **pending**. The
    gallery now uses this test build, and next is the native-noise switch
    (fc#16).
- **Local gate.** Two tests fail on this machine since the afternoon of
  2026-11-11, on `8df943d` too, which had passed: `mcp
  concurrent_replies_never_interleave` (broken pipe) and `subcommands
  typewrite_refuses_an_output_name_it_cannot_pick_a_format_for`. Both are
  environmental (the handoff already listed them as known), and CI runs
  them green.

## Not yet done on GitHub (this session's permissions blocked GitHub writes)

- **PR #1061's body.** A ready version is at
  `/Volumes/YOTUO/dev-cache/pscat_gpu/tmp/body1061.md`. It has `Closes`
  #1101 and #1102, `Refs` #1100, bullets for each, and the Dyn -0.0 note in
  Review.
- **Labels.** Move #1101 and #1102 to `in-review` (they close with
  #1061), and #1100 to `in-review`, leaving it open for the items above.

## Next executor levers (only if wanted)

- **Exact vector noise.** fractal_clouds' cost is mostly noise and the
  block loop, not trigonometry.
- **Divergence.** 8.3–12.3 of 16 lanes are active on average. Coherence or
  compaction are the candidates.
- **Moves.** Coalescing them is worth about 3%.
- **`Gather1`** is still scalar inside its handler.
- **`pow`/`ln`/`log` vectorised.** They are table-driven, about 2–3% of the
  pieces measured.
- What was learnt is in `notes/1101-faster-executor.md`: the cost is
  instructions per dispatch, and lane arrays get scalarised inside
  `Machine::block`. Keep new handlers' bodies out of `block`: inline, they
  made it 6% larger (#1100).

## Still to do before #1061 merges

1. fractal_clouds' validation of the test release.
2. The GitHub edits above.
3. The owner's merge of #1061. Then delete the remote
   `feature/compute-kernels-*` branches and confirm the issues closed.

## How to test against fractal_clouds

- **Temp files go on external storage,** under
  `/Volumes/YOTUO/dev-cache/pscat_gpu/`, not `/tmp` or the session
  scratchpad (the user's rule; the scratchpad was wiped mid-session).
  `env.sh` there sets `TMPDIR`. The test kit is in `kernels/`:
  - `bench.sh PSCAT W H SCENE...` renders compiled and `/interpret`
    versions, then runs `cmp` and prints compile, run, total time and RSS.
    Run it from a fractal_clouds checkout made with `git -C
    ~/dev/fractal_clouds archive <commit> | tar -x -C <dir>`, plus `mk.sh`
    and `mk2.sh` copied into its `renders/k117b/`. `NOINTERP=1` reuses
    existing references.
  - `samelook.py a.png b.png [diff.png]` is fractal_clouds' SAME-LOOK
    metric: PSNR ≥ 40 dB, p99.9 ≤ 8, mean ≤ 1.5.
  - `build1-747ccee/` and `build2-46ad11b/` are the builds fractal_clouds
    verified. build1's SHA-256 is `9adc198b…b4f`.
  - `ab/runk.sh BIN TAG SCENE W H` renders one compiled scene with
    `/usr/bin/time -l`. It prints the run, the steps, the instructions
    retired and the cycles.
    - The cycle counts are steadier than wall time on a loaded machine.
      The prelude alone (a 16×10 kernel) is about 72G cycles for
      golden_coast and 49G for mountains: subtract it.
    - `ab/baseline.sh BIN TAG` runs the whole #1101 acceptance set, and
      `ab/gal.sh` the gallery pieces.
    - `ab/fc9.sh BASE NEW TAG` renders the 9 fractal_clouds scenes at
      160×100 with both builds, then compares the PNGs and the step
      counts.
  - An x86_64 build runs here under Rosetta (SSE4.2, no AVX):
    `rustup target add x86_64-apple-darwin`, then build or test with
    `--target x86_64-apple-darwin`. Use a separate `CARGO_TARGET_DIR`.
  - `microbench/microbench.rs` is a ns-per-dispatch harness. Copy it into
    `src/kernel/` and add `mod microbench;` to use it, then remove it again.
- **Drivers** come from fractal_clouds' `renders/k117b/mk.sh` and `mk2.sh`:
  `mk2.sh SCENE NAME W H`. A `_interp` twin appends `/interpret
  .kernelmode` to line 1.
- **Always compare** against a fresh `/interpret` render from the same
  fractal_clouds commit.
- **fractal_clouds' bar is "looks the same"** (SAME-LOOK), not exact
  bytes. The user agreed that close output is fine when the deviation is
  understood and documented. But pscat's default path stays exact, so any
  approximation must be opt-in.
- **The fractal-clouds Claude session** has been the test partner. It runs
  the 8-scene check plus 640×400 and 1280×800 of mountains and golden_coast
  on any build path and SHA-256 you send it. It can also change
  fractal_clouds PostScript (the owner allowed it) when a branch won't
  compile. Its fixes so far: fc#142 (`FsMixWt`) and fc#143 (the trunk
  `aload`).

Environment variables: `PSCAT_KERNEL_TIMING=1` (timings, sizes, every
fallback reason), `PSCAT_KERNEL_DUMP` (the IR), `PSCAT_KERNEL_THREADS=N`,
and `/interpret .kernelmode` for the reference.
