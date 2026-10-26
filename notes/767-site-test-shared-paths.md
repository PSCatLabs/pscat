# #767: tests/site.rs flaked beside other runs

What failed: `tests/site.rs` runs `scripts/build_site.sh`, which does
`rm -rf _site` and rebuilds it. Two runs in one worktree (the gate and a
hand-run `cargo test`, or two agents sharing a checkout) delete each other's
output, and the loser fails inside the build, for instance
`site_examples: examples no page shows: 01-postfix, ...`. Reproduced on
base by starting two `cargo test --profile ci --test site` at once in one
worktree: one passed, one failed with that message.

What it was not: the test has no timeouts and reads no clock, and cargo's
"Blocking waiting for file lock" lines only delay a run. A `cargo build` in
another worktree with its own target dir did not make base fail: 3 single
runs of base in one worktree, with a rebuild loop (native and wasm) running
in a second worktree, passed 3 of 3. So the reported flake most likely came
from two runs overlapping in one worktree (#739 ran extra builds there).

Evidence, rounds of two concurrent `cargo test --profile ci --test site`
in one worktree:
- base: 3 rounds, run beside the rebuild loop: 3 of 6 runs failed, one per round;
- fixed: 3 rounds beside the rebuild loop, 6 of 6 passed; 3 more rounds
  before the loop got going (weak load), 6 of 6 passed.

Fix:
- `build_site.sh` takes `PSCAT_SITE_OUT` (output directory, default
  `<repo>/_site`) and `PSCAT_BIN` (a built binary, the name
  `scripts/typewrite.sh` and its siblings already use; skips
  `cargo build --release` here and in `build_font_gallery.sh`).
- The test builds into `common::scratch_dir("site")`, renders with
  `env!("CARGO_BIN_EXE_pscat")`, and removes the directory when it passes.
- CI and the Pages workflow set neither variable, so they behave as before.

Left alone: the wasm build still runs `cargo rustc` from inside the test and
copies the result to `web/pscat.wasm`, a path `tests/wasm.rs` shares. Cargo
runs test binaries one after another, so the two do not overlap in a normal
`cargo test`; two concurrent `cargo test` runs in one worktree could still
race on that copy. Making `build_wasm.sh` copy atomically is the follow-up.
