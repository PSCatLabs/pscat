# #703: bounded `--pstack-on-error` dump

**Problem.** `print_pstack` (src/main.rs) called `Object::repr()` on every
operand. `repr()` builds the whole string first and only caps nesting depth,
so a 1M-element array, a 100 KB string, a 1M-item stack, or a structure
whose shared references expand to 2^40 leaves printed without bound (or
never finished). MCP's 4 MiB cap then reported `output_too_large` instead
of the real error.

**Fix.** A bounded writer, `stack_repr_bounded` in src/object.rs, used only
by the dump. (#745 later gave `==`, `pstack` and `stack` the same writer with
larger limits: `notes/745-bound-operator-output.md`.)

| Bound | Value | Behaviour when hit |
|---|---|---|
| nesting depth | 32 | the container prints `... truncated` |
| elements per array | 256 | `... truncated` after the shown elements |
| bytes per string or name | 512 | cut before escaping, `... truncated` inside the parens |
| whole dump | 8192 bytes | bottom elements elided, `... truncated` leads |

- Elements are taken from the top of the stack down, so the newest operands
  survive a cut. The header still shows the true depth, `Operand stack (N):`.
- The byte check runs before each element, so output overshoots the cap by at
  most one scalar token. The work also stops there, so shared structure is
  never expanded.
- Under the limits the text equals `repr()` element by element. Ordinary
  dumps are byte-identical, pinned by `pstack_dump_small_output_is_pinned`
  (tests/cli.rs) and `under_the_limits_the_dump_equals_repr`
  (tests/pstack_dump.rs).
- The error line is printed before the dump and is not bounded.

**Not covered.** The `pstack`, `stack` and `==` operators (src/ops/misc.rs)
still print unbounded. A program's own output can still trip MCP's
`output_too_large`.

**Evidence.** Identity check (two binaries, `--loader lib/artkit.ps`): 101 of
101 page renders identical across 84 files, exit 0. It ran without a
`--control`, since a stderr-only change has no render-visible key. The two
binaries differ (cmp). `1000000 array foo` dumps 5,000,078 bytes on the base
binary and 1,372 on the head binary.
