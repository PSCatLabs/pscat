## `pscat chalk --hand` and `chalkboard_math` `hand` (issue #1111, 2026-11-19)

- **What.** drychalk's `/Hand` (#1049, PR #1076) is now reachable from the
  chalkboard front ends. Config key `chalk.hand` (0..1, default 0); CLI
  `--hand` (amount 1) and `--hand-amount A`; MCP `chalkboard_math` flat
  argument `hand` (a number 0..1; the `config` object takes `chalk.hand` too).
- **Byte identity.** The driver adds ` /Hand A` to the `ckink` dict only when
  the amount is above 0, so a board without it is the program it always was
  (checked with base and head binaries, see the PR). `--hand-amount 0` and
  `"hand": 0` are the plain board too.
- **Two flags.** `--hand` alone takes no value (a trailing LaTeX argument
  would be ambiguous: `pscat chalk --hand 1`), so the strength is a second
  flag, `--hand-amount`; it turns `--hand` on by itself, and either order
  works. Named "amount" because `chalk.strength` is the chalk's brightness.
- **Not the top-level `hand` key.** That one names the lettering face (only
  `"strokeglyphs"`, reserved). The freehand amount lives in the `chalk`
  group, mirroring drychalk's `/Hand`, and the MCP flat argument says so.
- **What it changes.** mathkit's glyphs come from `sgpath` strokes 10 to 50
  widths long, which drychalk's `/Hand` scales to a fraction of the full
  drift (notes/1049). Fraction bars, radical roofs, brackets and integral
  signs are long and visibly drift; ordinary letters barely move.
- **Left out.** `/Corners` (a box in four strokes) has no use in mathkit's
  output; the hosted pscat-mcp `chalkboard_math` follows the library on its
  next tag bump.
- **API.** `pscat::chalk::Config` (hidden, used by pscat-mcp) gains a public
  field `hand: f64`. A downstream `Config { .. }` literal without
  `..Default::default()` stops compiling on the next bump; `Config::default()`
  plus `apply`/`from_json` is unaffected.
