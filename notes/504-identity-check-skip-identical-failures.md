## identity_check.sh: a failure identical in both trees is SKIPPED (issue #504, 2026-10-14)

Friction from the #444, #488, #494 and #426 runs: a whole-corpus run (a `--files`
list of every tracked `.ps`) always exited 1, because
`docs/dev/design/typewriter/prototypes/tw_proto.ps` runs `twglyphs.ps`, which
`gen_glyphs.py` generates from a TTF and `.gitignore` keeps untracked. The file
fails the same way in the base and the head tree (`undefinedfilename ... run`,
line 26), and every agent had to explain the ERROR away.

Options weighed:

- **Commit `twglyphs.ps`.** Rejected: it is derived font software under the
  source's licence (OFL 1.1, and Liberation reserves its names), the generator
  and `.gitignore` deliberately do not track it, and a Liberation Mono copy
  would not be the Courier Prime that TYPEWRITER.md's timings and
  `prototype_vs_real.png` used.
- **Make the prototype skip when the file is missing.** Rejected: fixes one file;
  the next prototype with a generated input would bring the problem back.
- **Teach the script (done).** `compare_one` now reports `SKIPPED` instead of
  `ERROR` when a file fails identically in both trees: same nonzero exit status,
  same log (the scratch path masked, since pscat prints where it wrote the
  canvas), byte-equal canvases and page lists. Nothing about a failure that
  differs in any way changed: one tree only, another status, another message or
  another canvas is still an ERROR and still exits 1.

Guards, so a skip cannot hide a break:

- The SKIPPED line, a closing `WARNING:` on stderr listing each skipped file, and
  `, K skipped` in the summary line.
- If every file was skipped, nothing was compared: exit 1. Every file failing
  alike points at the harness (the binary, the page setup), not at the files.
- The negative control is untouched (exit 2 if it does not differ).

Evidence (unchanged tree, `--files` = every tracked `.ps`, 194 files): before,
exit 1 with 8 ERRORs; after, exit 0 with those 8 SKIPPED (tw_proto.ps plus
7 `lib/*.ps` files that are libraries, not drivers, and error when run alone,
identically in both trees). A control (an uncommitted draw appended to a
tracked example after its `showpage`, in the base tree only) exits 1 with
DIFFERS (page count 2 vs 1) and the same SKIPPED lines. `tests/identity_check.rs` covers the skip, the skip next to a real
difference, one-tree-only and different-status failures, and the all-skipped
case.

Gotcha: bash 3.2 (macOS /bin/bash) mis-parses a quoted literal containing a
variable inside `${var//"$OUT/base/"/x}`; put the pattern in a variable first.
