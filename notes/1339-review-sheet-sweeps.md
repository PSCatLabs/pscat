## review_sheet: sweeps, captions, mats and cols (issue #1339, 2026-11-19)

#744 put `pscat sheet` and `review_sheet` on one composition path, and #706 and
#1000 gave the CLI mats, `--cols`, sweeps and readable captions. The tool takes
the same now, all optional, so a call without them draws exactly what it drew
before (same child arguments, same `SheetRow`s).

**Inputs** (`src/mcp_server.rs`, schema and tool description):

- An entry's `sweep`: `{"seed": [1, 2, 3]}` or `{"param": "Density", "values":
  [10, 20]}`. The program is drawn once per value, every `dpi` item for each
  value in turn (the CLI's `Entry::plan` order), captioned `seed=7` or
  `Density=20`. The cells count toward the 32-cell cap.
- An entry's `captions`: one string per value in place of those; an empty one
  leaves the caption out. The count must equal the sweep's. A list, so no
  comma restriction as on the CLI.
- Top-level `values` (default true; the twin of `--no-values`) and `cols` (the
  wrap, 1..32).
- A `dpi` item's `mat` (`#rgb`, `#rrggbb`, `white`, `black`).

**How a cell renders.** Each cell is still its own limited child. A sweep value
is a one-frame sweep of the flag CLI (`--sweep-seed N`, `--sweep NAME=V`): the
CLI's own frame path, with the same sandbox, read roots and limits, and the same
"never called srand or sampled noise" warning (it reaches the reply's
`Warnings:`). A matted cell adds `--transparent`; the shared `compose_sheet` sets
it on the mat. `tests/mcp.rs` pins sweeps, captions, `values` and `cols` against
`pscat sheet` byte for byte, matted cells included (antialiased edges too): the
child hands its transparent frame over as a PNG, which stores a partly
transparent pixel un-multiplied, and that came back exact for the test's art. In
principle a very faint edge pixel could be a level off the CLI's; making that
impossible would need the child to take the mat itself (a new CLI flag), which
this ticket does not add.

**Decisions.**

- Sweep values are JSON numbers, not the CLI's `A:B:STEP` ranges. The range
  grammar and `format_sweep_value` are private to `main.rs`, which `pscat-mcp`
  does not link; moving them into the library would be a bigger change than
  this ticket. A number prints bare when whole (`2.0` is `2`), else in Rust's
  shortest decimal; that one text is both the caption and the value the child
  defines, so they cannot disagree. A value of more than 40 characters, a name
  that is not an identifier, and a caption count mismatch are refused before
  anything runs.
- A mat pads every side, so it counts toward the 8000 px side cap in the
  pre-check (`output_too_large`, `limit_side_px`), and the plan's own
  `CellTooLarge` is now the same structured error instead of `internal`.
- Stacked columns (`+`) are not exposed.

**Shared hidden items.** One added: `pscat::contact_sheet::parse_mat` (moved from
`src/subcmd/sheet.rs`, wording unchanged). Nothing existing changed: no
signature, field or ordering. `pscat-mcp` (the separate crate) can adopt the
same inputs on its side; the hosted ChatGPT skills do not mention them until it
does.
