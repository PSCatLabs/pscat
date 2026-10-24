## pscat chalk: raw LaTeX to a chalkboard (issue #555, 2026-10-23)

`pscat chalk 'x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}' -o board.png` writes a
formula in chalk on a board, from LaTeX as a user types it. The driver
generator is `pscat::chalk` (`src/chalk.rs`); `src/subcmd/chalk.rs` is a
thin CLI over `chalk::render`, so the CLI, the MCP tool (#688) and a browser
demo all draw the same bytes.

**Shape of the generated program.** handscript, strokeglyphs and mathkit
load first; the lines are laid out with `mkalign` at 0 0 with `/Ink { pop
newpath }` inside a `gsave`, and the union of the returned records' `/BBox`
is the block. If it is wider or taller than the writing box (the page less
the margins and the board's ledge), the size is scaled down and the block
re-measured (up to four times; the layout is close to linear in `/Size`).
Only then do artkit, paintkit, drykit and drychalk load and `dcboard` run,
so a TeX error costs a mathkit load, not a board. The records' boxes come
from the layout, not the ink, and with a fixed `/Seed` mathkit's layout does
not depend on the position, so measuring at 0 0 and drawing at the offset
place the same block. Everything uses one `mkalign`, even a single line.

**Input.** Comments, newlines and a display wrapper around the whole are
removed (`$`, `$$`, `\[ \]`, `\( \)`, and `align`/`align*`/`aligned`/
`gather`/`equation`/... only when its matching `\end` closes the input: a
nested `aligned` stays). `\label{}`, `\notag` and `\nonumber` are dropped.
Rows split at `\\` (with `*` or `[2pt]`) only at brace depth 0 and outside
any environment, so `pmatrix`, `cases` and `\substack` keep theirs. Each row
becomes a PostScript string with `\`, `(`, `)` escaped, so single-backslash
input reaches mathkit whole (`\nabla`, `\theta`, `\beta`, `\rho`, `\frac`
are the ones a raw string would eat) and no input can end the string.

**Errors.** mathkit raises `undefined` on a name `mk-<kind>:<token>`;
`chalk::explain` turns it into `ChalkError::Latex { kind, token }` with a
one-line message and a stable `code()` (`unsupported_command`, ...) for
tools. Nothing is written on an error.

**Defaults, from looking.** An 800x400 slate at 144 dpi, size 64, chalk width
0.048 of the fitted size (the lecture piece's ratio). Ghosting picks two or
three stock formulas from the seed and puts them along the top and bottom of
the board, where a centred formula seldom is; in the middle they crowded
the writing. Fit only shrinks: the size is a writing size, and a short
formula on a big board is what a board looks like.

**No board.** `board: false` sets a bare `dksurface` on a transparent page
and gives `dcchalk` the chalk's own colour as `/Under`, so every mark is the
stick's colour and only the coverage varies (no dark fringe on a light
page). With no grey levels the default touch looked like marker, so the
default `pressure` is 0.34 there (0.68 on a board); an explicit pressure is
used as given.

**Vector output is big.** A full board is about 20 MB as SVG and 5 MB as
PDF (every catch is a small fill, and the haze an image); the guide says
to prefer PNG.

**Tests.** `tests/chalk.rs` pins the issue's five formulas (quadratic, an
integral with limits, a `pmatrix`, a `cases`, a two-row `align`) on a
400x200 board at 72 dpi by an FNV hash of the pixels. Any change to
mathkit, strokeglyphs, drychalk, drykit or the generator that moves a pixel
moves them: look at the boards, then re-pin from the failure message. macOS and Linux agree on all five; Windows renders the `cases` board a
few pixels differently (its C runtime's transcendentals round differently
in the last place), so that one has a Windows pin of its own.

**Deferred.** A web demo page (the generator is wasm-safe, but no wasm
export is wired), configurable hands (`hand` is reserved), and the MCP tool
(#688).
