# chalk: centre `gather` rows like LaTeX (#699)

`pscat chalk` laid every multi-row input out with one `mkalign`, which puts
each row's `&` (or, with none, its first relation) on one column. That is
`align`. LaTeX's `gather` centres each row on its own width, so pasted
`\begin{gather} a = b \\ ccc + dd = e \end{gather}` came out as a
relation column instead.

**What changed**
- `lib/mathkit.ps`: `mkalign` takes `/LineAlign` (`/relation`, the default, or
  `/center`). `/center` sets every line's alignment point to half its final
  width, after `mkcolumns` has widened any `&` columns, so the line sits
  centred at `x`. Any other value raises
  `mk-linealign-must-be-relation-or-center`. Unset, nothing moves.
- `src/chalk.rs`: `layout()` is `lines()` plus a `RowAlign`; `lines()` stays
  the public wrapper. The innermost stripped environment that sets rows
  decides (`equation`, `displaymath` and `math` only hold a display):
  `gather`, `gather*` and `gathered` centre; `align`, `aligned`, `split`,
  `multline`, `flalign`, `eqnarray*`, or no wrapper, align on the relation.
  `gather` around `aligned` is therefore `align`; `aligned` around `gathered`
  is `gather`. The driver adds `/LineAlign /center` to the options of the
  measuring pass and the drawing pass alike, and only for more than one row
  (a single row is centred by the block, and its driver stays the old one).
- `multline` (first row left, last right, the rest centred) is not done.

**Why mathkit, not chalk alone.** One `mkalign` per row, offset by the row's
measured width, would also centre it, but `mkalign` seeds line `i` from its
index (`Seed + 101 i`) and sets each baseline from the depth of the line
above. One call per row would give every row index 0 and no leading, so the
irregularity and the spacing would change, not just the x. Hence an option.

**Identity.** `align` and every golden hash are unchanged; the option is off
unless a `gather` wrapper asks for it.
