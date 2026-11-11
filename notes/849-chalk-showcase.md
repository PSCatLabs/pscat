## README + site: the `pscat chalk` showcase (issue #849, 2026-10-26)

`pscat chalk` / `chalkboard_math` was documented only in the dry-media and
agents guides. Now the front page has a "LaTeX on a chalkboard" section with
four boards (the quadratic, an aligned derivation, a pmatrix in yellow, and
`--no-board` chalk over a dark ground), each with the exact command under it,
and the README has the quadratic board, its command, a link to the dry-media
guide section and a line on the MCP tool.

- **Built, not committed.** `site/chalk/boards.json` is the list;
  `scripts/site_chalk.py` renders each board with the build's binary and
  expands `<!--@chalk-showcase-->` in `site/index.html`. The command shown is
  built from the same argv (`shlex.quote`), so picture and command cannot
  drift. Renders took ~1 s each in a release build; the site build gains
  about 4 s.
- **README image** is an absolute URL to `assets/chalk/quadratic.png` on the
  Pages site (pages.yml deploys every push to main). No PNG is added to the
  repo's README path or to Cargo's `include`. The image exists only after the
  first deploy that includes this change.
- **Rendered after #778 and #731**; the review-log images of #555 predate
  them and were not reused.
- **Not done:** `docs/dev/design/drymedia/chalk_cli_demo.png` (the dry-media
  guide's image) is from before those changes; re-rendering it is a
  follow-up. #557 (a live LaTeX demo page) can reuse `boards.json` as its
  preset list.
