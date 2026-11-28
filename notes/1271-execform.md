## execform and the Form resource category (issue #1271, 2026-11-28)

`execform` (`src/ops/graphics.rs`, `Frame::Form` and `begin_form` /
`finish_form` / `abort_form` in `src/interp.rs`) and the `Form` category's
`InstanceType` check (`defineresource`, `src/ops/level2.rs`). The reference
is Ghostscript 10.07.1's own definition (`Resource/Init/gs_lev2.ps`,
`.execform1` and `execform`), run for run in `tests/execform.rs` (120 cases,
run by pscat and by `gs`) with the pictures (three scales, a form twice, a
rotating `/Matrix`, a `/BBox` clip) held against gs's raster.

**What it does.** `form execform -`: inside a `gsave`, in gs's order,

1. `/FormType` must be 1 (or 1.0); anything else, or none, is `undefined`.
2. `/Matrix` is `concat`ed (`rangecheck` unless six, `typecheck` for a
   non-number or a non-array).
3. `/BBox` is `[llx lly urx ury]` and clips as `llx lly (urx-llx) (ury-lly)
   rectclip` does, so an inverted box is the same rectangle. gs runs
   `aload pop exch 3 index sub ...` on whatever the array holds, so a box of
   other than four numbers is `typecheck`, or `stackunderflow` for none or
   one element when the stack under the form is too shallow for the `3 index`
   (`below + 2 + n < 4`); the table pins each.
4. `/PaintProc` is any object, `exec`ed (a literal is pushed, a name is run).
5. A form without `/Implementation` gets `/Implementation null`, put even in
   a read-only dictionary and journalled for `restore`. The PLRM reserves the
   key. A form that already has a non-null one fails as gs does (a dictionary:
   `undefined`, anything else: `typecheck`): gs looks for a cached form there.
6. The PaintProc runs with the form on the operand stack **twice**, as gs
   gives it; `/XUID` is not read.

When it returns, gs's clean-up of what it left: scan down for the first
dictionary that has `/Implementation` (the form), remove it and everything
above; none found, remove the top operand (gs's `pop pop`). So `{pop}` and
`{begin ... end}`-style procedures that take their one operand leave the
stack as it was, `{}` leaves the form, and a PaintProc that pushes junk loses
it. Then the `grestore` operator itself (with its save boundary, so a
PaintProc's own stray `gsave` or `grestore` does what gs does).

**Errors put everything back.** A failure in the setup, the PaintProc, a
`stop`, a limit or an abort restores the operands and the graphics state, and
a refusal for room or limits comes before anything is changed, the form's own
`/Implementation` included. The operands are the top 256 of the stack as
`execform` found it, the form included. The copy is put back by cutting the
stack to the operands under it and putting the copy on top, so it is exact
unless the PaintProc changed the stack below the copy: one that pops deeper
and ends shallower than the operands under the copy is left as it left them,
and one that pops below the copy and pushes the stack back above it ends with
the first operands it left plus the copy (`tests/execform.rs` pins both). A
stack of 256 or fewer is always restored whole; nothing cheaper tracks how
low the PaintProc took the stack between steps.

**Deliberate differences from gs** (not in the table; tests of their own):

- gs puts back the *count* only: after an error past its internal `matrix
  currentmatrix` the form is replaced by the saved CTM, `{pop pop pop 1 0
  div}` leaves `[7 8 8]`. The PLRM says the operands are unchanged, and
  that is what pscat does.
- gs names `--.execform1--` as the offending command for most errors (its
  operator is a PostScript procedure); pscat names `execform`.
- gs's clean-up leaves counters on the stack when a dictionary without
  `/Implementation` lies in the scanned region (`{pop 1 dict}`); pscat skips
  such a dictionary.
- gs gives `limitcheck` for a `/BBox` of `1e300`; pscat's `rectclip` (and so
  `execform`) clips to it. That is `rectclip`'s existing difference, not new.
- A read-only or `noaccess` *dictionary* and a `noaccess` array are not
  modelled (#1189 models write access for arrays only), so gs's
  `invalidaccess` for them is not reproduced.
- gs's `restore` inside a PaintProc of a `save` made outside is
  `invalidrestore` (the form is newer than the save); pscat's `restore`
  does not look at the operand stack, with or without a form.
- No form caching (`.beginform`, `.get_form_id`, `/FormID`), no
  `UNROLLFORMS`.

**Limits.** At most 64 forms open at once (the depth a pattern's `PaintProc`
has for captures); the 65th is `execstackoverflow`, as the recursion is in
gs (at its own 308). Each open form holds a page-sized clip mask: the mask
is asked of `--max-memory` before it is made, and past four forms open the
pixels of all the open masks are held to the pattern code's nested bound
(134 million, `limitcheck`). A page is at most 8000 pixels a side (pscat's own
cap, 64 million pixels), so the floor of four keeps a valid form in a form
working on any page (four masks are at most 256 MB) and is the most a
self-painting form is let take. At 300 and 600 dpi it stops after a few levels (peak RSS 170 MB and
280 MB) instead of gigabytes. The operand copy, the journal's copy of the form
dictionary (inside a `save`), the copy of the current path the `gsave` makes
and the clean-up scan are charged to `--max-steps`, `--max-time` and
`--max-memory` before they are made, the scan in chunks of 1024 operands.

**`Form` category.** `defineresource` with the `Form` category is a
`typecheck` for a non-dictionary instance, as in gs (`InstanceType
/dicttype`); the operands are put back. `findresource`, `resourcestatus` and
`undefineresource` were already generic and work for forms.
`resourceforall` does not exist in pscat yet (#1218 owns it); once it does it
lists forms through the same registry. Not done: gs makes the instance
read-only on `defineresource` (dictionary access is not modelled), the
`/Form /Category` resource (`resourcestatus` is `0 -1 true` in gs, `false`
here; #1218 lists the built-in instances), and the `FormType` category.
