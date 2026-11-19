# `pscat fonts list | head` and sweeps on a closed stdout (#1117)

#1069 made a closed stdout (`| head`) a quiet exit 141 for the main CLI, with
its own `println!`/`print!` in `src/main.rs`. Two paths were left:

- `pscat fonts ...` lives in the library crate (`src/fontpacks/cli.rs`), so it
  used std's `println!`, which panics on `EPIPE`. It now has the same shape
  locally: a `println!` that remembers the closed reader and drops the text,
  and `main` returns 141 instead of reporting. `fonts list` also skips
  verifying the remaining packs once the reader is gone. No new public item.
- A sweep frame whose program wrote to the dead pipe reported `ioerror` and
  marked the frame failed. The frame now takes the same arm as a plain render
  (`stdout_epipe_error`): no report, not failed, its PNG written. The loop then
  stops (it also stops when the sweep's own progress line meets the closed
  pipe). A requested `--contact-sheet` is still written, with the cells that
  rendered, as a plain render still writes its page.
