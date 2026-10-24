# MCP `chalkboard_math` (#688)

`pscat-mcp` has a fifth tool: LaTeX in, a chalkboard PNG out. It wraps the
#555 driver (`pscat::chalk`), so an agent no longer has to write PostScript
with every backslash doubled to get a formula on a board.

**What changed**
- `src/mcp_server.rs`: `chalkboard_math`. Arguments: `latex` (required), the
  #555 config keys as flat optional arguments (`size`, `irregular`, `slope`,
  `jitter`, `align`, `color`, `board`, `ghosting`, `seed`, `width`, `height`,
  `dpi`), `format` (png, svg, pdf), `config` (the full `pscat chalk --json`
  object, with the flat keys laid over it) and `out_path`. Annotations:
  `readOnlyHint: false`, like the other two render tools.
- `tests/mcp.rs`: four tests. The quadratic formula at seed 1 has the pixel
  hash that `tests/chalk.rs` pins, and its bytes are what `chalk::render`
  returns. The other tests cover a transparent board, the `\oplus` error,
  the caps, bad arguments, and the step limit.
- Docs: `docs/guides/agents.md` (the use-case table sends "formula on a
  chalkboard" to the tool; tool list, annotations table, limits),
  `SECURITY.md`, `docs/STABILITY.md`, `docs/dev/HANDOFF.md`,
  `site/agents.html`, the pscat skill and psart's `board-math.md` (one line
  each), and `scripts/mcp_inspector_check.sh` (tool names).

**How it runs.** The issue asks for the driver to be called directly. The
#604 rules ask for the same limits and isolation as `render_postscript`. The
tool meets both: the server generates the program in-process with
`chalk::driver` (pure, no file or environment access) and pipes it to a
`pscat` child (`--page WxH --dpi D [--transparent] --png out -`) through
`run_child` with `interpreter_args`. The result is byte-identical to `pscat
chalk`. Rendering in-process with `chalk::run` would skip the child's
step/time/memory limits, the timeout kill and crash isolation.

**Errors.** When the child fails, the order is: a `limit` field gives
`limit_exceeded`. Otherwise an `undefined` whose offending command is
`mk-<kind>:<token>` goes back through `chalk::explain` and comes out as
`kind: latex_error` with `code` (`unsupported_command`, ...) and `token`,
one line, and no image. The partial page is blank anyway, because the error
comes from the measuring pass. Anything else is `render_failed`. A config
error names the flat key the caller sent (`size must be ...`, not
`layout.size`).

**Caps** (fixed constants, checked before the child starts):
- `latex` at most 4096 bytes. A ten-row derivation is about 1 KB. Render
  time grows with length (1 row: 1.9 s, 10 rows: 5.6 s, 40 rows / 4 KB: 25 s
  on the default board, release build, this machine), so 4 KB is four times
  a long derivation and still inside the default 55 s interpreter clock.
- The board at most 40,000,000 device pixels, and 8000 a side, rounded the
  way `Interp::with_page_scaled` rounds. Resident memory is about 17 bytes a
  pixel (1.3 Mpx: 0.11 GB; 5.8 Mpx: 0.24 GB; 36 Mpx: 0.66 GB, 8 s; 40 Mpx
  under the MCP limits: 0.63 GB, 12 s). The canvas is allocated before
  `--max-memory` starts counting, so the cap is what keeps a big board in
  bounds. 40 M covers an 8K screen and an A2 poster at 300 dpi.
- svg and pdf need `out_path`: the default board is 20 MB of SVG (9 MB at
  400x200 pt, 16 MB without a board) and 5 MB of PDF. Every grain mark is a
  path, so neither fits inline.

**Judgment calls**
- `readOnlyHint: false`, although the issue says "true when there's no
  `out_path`". Hints are static per tool (#687), so a tool that takes
  `out_path` gets the conservative value, as `render_postscript` and
  `handwrite` do.
- `ghosting` together with `board: false` is refused. `Config::apply` would
  otherwise bring a default board back without saying so.
- `board` also takes `"slate"`, `"greenboard"`, `[r, g, b]` or a board
  object, as well as `true`/`false`.

**Eval row for #623** (no eval-set file exists yet; posted on #623 too):
prompt "Put the quadratic formula on a chalkboard for my slides", expected
tool `chalkboard_math` with `latex` `x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}`.
Expected output: one PNG, white chalk on slate. A miss is a
`render_postscript` program, or LaTeX with doubled backslashes.

**Not done**
- `render_postscript` and `handwrite` have no pixel cap below the
  interpreter's 8000 px a side (64 Mpx, over 1 GB). Proposed as a follow-up.
- `plugin/README.md` still lists four tools. Left for #686, which rewords
  that file.
