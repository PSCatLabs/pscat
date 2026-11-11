# Handoff: compute-kernels epic (2026-11-11)

Session: https://claude.ai/code/session_016y8NtRr7b9NWRg1SctzFfp. It wrote
both the 2026-11-10 handoff and this update. The 2026-11-09 handoff was
session_016y8NtToNBqdyScG3LMGriu.

## Where things stand

- **The epic.** `feature/compute-kernels` is pushed. It is
  PR PSCatLabs/pscat#1061 (open, owner-merge, because it touches
  `Cargo.toml`). Every CI workflow on `46ad11b` is green.
- **#1101 (faster executor) is done.** It was merged into the epic at
  `46ad11b` from `feature/compute-kernels-1101-executor`. Details and
  numbers are in `notes/1101-faster-executor.md`.
  - The executor runs a flat 16-byte lowered form (`kernel/code.rs`), with
    branch-free vectorised fallible ops, blend and full-width stores
    (`Kernel::fresh`), `Gather1`, add/sub/mul opcodes and 8×2 batches.
  - The bytes and step counts are unchanged. An independent review found
    one bug, an overflow panic under overflow checks in `Gather1`. It is
    fixed, with a test.
  - fractal_clouds' check (fc#117 comment 6793642064, pscat#1101 comment
    6793646852) found every render byte-identical. golden_coast is 1.56×
    faster with the builds interleaved: 1280×800 from 22.1 to 14.1 s, and
    the 2× yardstick (a 2560×1600 kernel) from 84.9 to 54.3 s.
    fractal_clouds is satisfied, and its `fastrender.sh` now defaults to
    the CI build below.
  - **Not yet done on GitHub** (this session's permissions blocked GitHub
    writes):
    - add `Closes #1101` to #1061's body, plus a line on the executor;
    - move #1101 from `in-progress` to `in-review`, with a comment naming
      `46ad11b`.
- **What the PR closes.** It lists `Closes` for #742, #1062–#1066,
  #1091–#1094 and #1099 (and #1101 once added). It lists `Refs` for #1044
  and #1100. These issues close when #1061 merges.
- **CI artifact.** The release dry run on `46ad11b` is run 43000416269,
  green. Its artifacts expire 2027-02-09. The manifest's commit is
  `809f906`, the PR merge onto main.
  - aarch64-apple-darwin tar.gz SHA-256:
    `5e1b0cbf454ba2f0be2b11b07b338ef54f2899f9ff8413531a357ca2b08be52d`.
  - It renders mountains and golden_coast `cmp`-identical (checked here and
    by fractal_clouds).
  - The local build of the same commit is
    `kernels/build2-46ad11b/pscat` (SHA-256 `47238116…7972`).
- **Earlier follow-ups** (#1091, #1093, #1099, compile time) are in
  `notes/1091-1093-1099-kernel-followups.md`. The purity forms "told from
  the text" were shown unsound. Don't retry them.

## Next executor levers (only if wanted; fractal_clouds asks for nothing now)

- **Divergence.** 8.3–12.3 of 16 lanes are active on average, so about a
  third of the lane-work is masked. Coherence or compaction are the
  candidates. `PSCAT_KERNEL_TIMING=1` prints the figure (`kernel batches:`).
- **Moves.** They are 19% of dispatches. Coalescing in the allocator is
  safe only in the plain case, worth about 3%.
- **`Gather1`** is still scalar inside its handler.
- The rest of what was learnt, so it isn't re-measured, is in
  `notes/1101-faster-executor.md`:
  - The cost is instructions per dispatch, not memory. The register file
    size does not matter.
  - By-value lane arrays get scalarised inside `Machine::block`.
  - Out-of-line handlers are slower.

## Still to do before #1061 merges

1. **Codex review of the whole branch** (the user's plan), now that Codex
   is back after Nov 11 9:34 AM. Use `/codex:rescue` or the codex plugin's
   review path, against `main...feature/compute-kernels`. It now includes
   #1101.
2. **Full `cargo test --profile ci`.** CI on each push runs it. Check
   the `ci` run for the latest push. The Windows budget flake is fixed. Two known failures are not from this work: a
   typewrite test that also fails on clean main, and the MCP concurrent
   test, which depends on the environment.
3. **Owner merge of #1061** (CODEOWNERS path). Then delete the remote
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
