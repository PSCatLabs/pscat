<!-- cloud: sub "A single formula on a plain board needs no PostScript: `pscat chalk
'x = \frac{1}{2}' -o board.png` (single backslashes; see the pscat skill's `references/handwriting-chalk.md`),
or the MCP tool `chalkboard_math` when it is connected." -->
A single formula on a plain board needs no PostScript: call `chalkboard_math`
with `latex` set to `x = \frac{1}{2}` (single backslashes; see the pscat skill's
`references/handwriting-chalk.md`).
<!-- cloud: sub "`pscat chalk` sets it for a
  `gather`" -->
`chalkboard_math` sets it for a
  `gather`
<!-- cloud: drop "`pscat chalk --print-ps 'a &= b \\ c &= d'` prints the complete PostScript" -->
<!-- cloud: sub "`pscat chalk --help` prints the whole list. Anything else fails with a name" -->
Anything else fails with a name
<!-- cloud: sub "In `pscat chalk`, a bracket" -->
In `chalkboard_math`, a bracket
