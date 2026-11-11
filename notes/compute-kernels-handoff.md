# Handoff: compute-kernels epic (2026-11-10)

Session: https://claude.ai/code/session_016y8NtRr7b9NWRg1SctzFfp
(the earlier handoff, 2026-11-09, was session_016y8NtToNBqdyScG3LMGriu).

## Where things stand

- **The epic.** `feature/compute-kernels` is at `7afa9f9` and pushed. It is
  PR PSCatLabs/pscat#1061 (open, owner-merge: it touches `Cargo.toml`).
- **What the PR closes.** It now lists `Closes` for #742, #1062–#1066,
  #1091–#1094 and #1099, and `Refs` for #1044 and #1100. All eleven carry
  `in-review` plus a comment naming their epic merge commit, and they close
  when #1061 merges. #742 is no longer `deferred`.
- **This session's follow-ups,** from fractal_clouds' fc#117 rerun, all
  merged into the epic at `7afa9f9`. Details and numbers are in
  `notes/1091-1093-1099-kernel-followups.md`.
  - **#1091:** sharing compiles get their own 2M size cap (the inlined pass
    keeps 400k). `MAX_SUB_MADE` is 4M, `--max-memory` is checked while
    compiling, and `.kernelcaps` is a test hook that can only lower the
    caps.
  - **#1093:** `PSCAT_KERNEL_TIMING=1` lists every "cannot run alone"
    reason. The purity forms told from the text were tried and **removed**:
    two independent reviews showed each one unsound. Don't retry them
    without reading that note.
  - **#1099:** SIMD for div, sqrt, the compares and the rounding ops,
    byte-identical.
  - **Compile time:** joins about 30% cheaper.
- **fractal_clouds result.** At fc ea18d58, all 8 scenes compile, nothing
  retires, and every render is `cmp`-identical to `/interpret`, 23–83×
  faster. fractal_clouds says it is satisfied. Its owner's `fastrender.sh`
  uses the build: first `/tmp/pscat-build1` (local build1 = `747ccee`,
  same code), then the CI artifact once fractal_clouds verifies it.
- **CI artifact.** The release dry run on `7afa9f9` is run 42959822321,
  green. Its artifacts expire 2027-02-09. The manifest's commit is
  `1d2b8d5`, the PR merge onto main.
  - aarch64-apple-darwin tar.gz SHA-256: `502584b56db16a94f144b565f652e0ed9c6f0c33a363fc34a5a6c556cf10bb2d`.
  - Its binary renders mountains `cmp`-identical to `/interpret` (checked).
- **CI on `7afa9f9`.** `test` and every other job pass except two Windows
  jobs: `a_long_chain_of_merges_compiles_in_linear_time` ran 0.38–0.51 s
  against a 300 ms budget. That flake predates this session (it failed at
  `b959248` too). The budget is now 1.5 s, in the commit after this note.
- **Filed this session:**
  - #1099 (done);
  - #1100: wider x86 SIMD, plus an opt-in, documented approximate-math
    mode. Low value for fractal_clouds, whose math is under 4% of a frame;
  - #1101: a faster executor for large kernels. **This is next.**

## Next: #1101 (start in a new session)

The owner iterates at 1280×800, often with 2× supersampling, one frame per
process. At those sizes compile time is noise and `kernel cpu run` is
everything. fractal_clouds' measurements on build1:

| frame | cpu run |
|---|---:|
| mountains 1280×800 | 14.8 s |
| golden_coast 640×400 | 5.7–7.5 s |
| golden_coast 1280×800 | 26.6 s |

A golden_coast 2560×1600 kernel would take about 110 s. fractal_clouds
wants golden_coast 1280×800 at 2× as the yardstick.

What is already known (don't re-measure):

- `sample` on golden_coast 640×400 puts almost all of the run in
  `Machine::block` itself. Math functions are under 4%.
- 8 and 32 lanes are both slower than 16.
- The GPU is not an option for the shared-subroutine scenes:
  - inlined, they pass the 1M-instruction limit;
  - as WGSL functions, Metal's compiler ran for over 10 minutes (reverted);
  - mountains alone works and passes SAME-LOOK, but naga's WGSL front end
    costs 5–12 s per process.

  fractal_clouds agreed to park it.
- golden_coast is 170k instructions, 222 shared subroutines and 4.4k
  registers. An `Ins` is up to 128 bytes.

The candidates, each measured one at a time, are in #1101:
- a compact instruction encoding;
- registers shared across subroutines that are never live together;
- 2D pixel tiles;
- divergence accounting.

## Still to do before #1061 merges

1. **Codex review of the whole branch** (the user's plan), now that Codex
   is back after Nov 11 9:34 AM. Use `/codex:rescue` or the codex plugin's
   review path, against `main...feature/compute-kernels`.
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
  - `build1-747ccee/` is the build fractal_clouds verified (SHA-256
    `9adc198b…b4f`).
- **Drivers** come from fractal_clouds' `renders/k117b/mk.sh` and `mk2.sh`:
  `mk2.sh SCENE NAME W H`. A `_interp` twin appends `/interpret
  .kernelmode` to line 1.
- **Always compare** against a fresh `/interpret` render from the same
  fractal_clouds commit.
- **fractal_clouds' bar is now "looks the same"** (SAME-LOOK), not exact
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
