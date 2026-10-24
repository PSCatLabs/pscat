## Labelled review sheets with mixed cell sizes: `pscat sheet` (issue #693, 2026-10-23)

The #683 logo agent needed four candidates, each at 512 px and 48 px, side by side and
named, and `--contact-sheet` could not do it (same-size sweep frames, no captions), so the
one non-pscat image in that deliverable was the sheet, made in PIL. `pscat sheet` makes
that sheet in one command:

    pscat sheet -o sheet.png --page 384x384 --dpi 96,9,9x4 \
        --label "A: one-line cat" a.ps --label "B: drawing board" b.ps ...

**Why a subcommand, not new `--contact-sheet` flags.** `--contact-sheet` is an output of a
sweep: one source, one `--page`/`--dpi`, N frames. Mixed files, per-cell dpi and captions
are a different input model, and fitting them in would have meant growing `Options` and
`parse_args` for flags that only make sense together. `src/subcmd/` already has the pattern
(`pscat chalk`). The old path is not touched, so its output is byte-identical by
construction (and checked: the PR has the comparison).

**Model.** Each file is a row; each `--dpi` entry is a cell. `--page` and `--dpi` are sticky
for the files after them (one sheet can mix a 384 pt icon and a letter page); `--label`
applies to the next file. A cell is drawn at its own true pixel size, centred in a column
as wide as its widest cell or caption, with a 1 px grey hairline *outside* the image so a
pale render does not vanish on the white sheet. Under each cell is its pixel size
(`512x512 px`), above each row a label (default: the file name). An entry like `9x4` is the
9 dpi render enlarged by an exact integer factor with nearest-neighbour sampling, captioned
`(shown at 4x)`; it is opt-in, so "nothing scaled" holds unless asked for.

**Code.** `contact_sheet::Layout` is pure geometry over sizes (so the sheet's size is known,
and checked against the 8000 px side limit, before any cell renders); cells then stream into
the sheet one at a time like the sweep's frames. `contact_sheet::render_text` sets labels
with pscat's own text rendering: it runs `/Helvetica findfont ... stringwidth` through an
`Interp` to measure, then draws on a page of exactly that size. Characters outside printable
ASCII show as `?` (the standard Latin encoding): a non-Latin label is a gap, see below.

**Not done** (proposed follow-ups in the PR): seed and parameter sweeps inside `sheet`
(labelled with the seed or value), a `--cols` wrap for rows wider than the side limit, a
dark or transparent sheet background, `--halftone`, non-Latin labels, and an MCP tool.
