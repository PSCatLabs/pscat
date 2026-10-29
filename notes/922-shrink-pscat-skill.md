## Shrink the pscat SKILL.md to the common path (issue #922, 2026-10-29)

`plugin/skills/pscat/SKILL.md` loads in full on every request, and the common
task (render a file) needs a few lines. It had grown to 457 lines / 3879 words
with `--bands`, `--profile`, sheets, typewriter effects, noise, `readreals`
and the MCP job tools. Now 237 lines / about 1940 words.

- `SKILL.md` keeps: install and locate (#919/#920), render with the page-size
  rule (#921/#909), "Show the result to the user" (#911, unchanged, in the main
  file on purpose), debug and lint, fonts, the output-root lines of the MCP
  paragraph (#910), pitfalls, and an index that says when to open each
  reference file. The handwriting "no real person's hand" rule stays in the
  index line, not only in the reference.
- `references/`: `bands-profile.md`, `sheets.md` (sweeps, contact sheet,
  `pscat sheet`, `review_sheet`), `handwriting-chalk.md`, `typewriter.md`
  (typewrite and released documents), `extensions.md` (setalpha, noise,
  `writereals`/`readreals`, photo-etch, the window, `--capabilities`), `mcp.md`.
- `tests/skill_blocks.rs` already walked every Markdown file under
  `plugin/skills`; its must-have list now names `references/extensions.md`
  (the pscat skill's only runnable fence moved there) and a new test fails on
  a pscat reference file that `SKILL.md` does not link.
- psart's two "see the `pscat` skill" pointers and `board-math.md` now name the
  reference file. Their own length is out of scope.
- Checked that nothing was lost by comparing the old file's backticked tokens,
  numbers, code lines and sentences with the new corpus; the residue was
  headings and the sentences deliberately reworded for the split.
