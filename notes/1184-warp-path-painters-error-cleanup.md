## warp: a failing warp no longer corrupts the stack under fill, stroke, clip and the rect operators (issue #1184, 2026-11-12)

#1179 guarded the show family with `wp--guarded`. The path-painting
decorators (`fill`, `eofill`, `stroke`, `clip`, `eoclip`, `rectfill`,
`rectstroke`, `rectclip`) still ran the warp bare: when the user's warp
raised an error, a program that caught it with `stopped` got back the saved
CTM and `/m` (the warp's operands), the CTM left at the run's base, a
flattened or stroked path, `clip`'s saved path records, and, for the rect
operators, a leaked `gsave` and the rect path.

- **Compute, then apply.** `wp--rebuild` is split. `wp--compute` only reads
  the path and returns the warped records; `wp--apply` makes them the path.
  `wp--try ( saved region -- result )` runs the compute step inside
  `gsave ... stopped`, then `grestore`s on success too, so the user's warp
  can only ever touch a graphics state that is thrown away. On an error it
  clears the stack back to a mark, undoes the `gsave` (CTM and path come
  back with it), pushes the operands in `saved` again and re-raises with a
  bare `stop`, so `$error` keeps `undefinedresult` / `div`.
- **Success path unchanged.** The rebuild (`newpath`, emit, `setmatrix`)
  runs after the guard, with the same operations in the same order, so
  gallery renders are byte-identical (`identity_check.sh --loader
  lib/warp.ps`). `clip` and `rectclip` could not be guarded by a `gsave`
  around the whole call: they must keep the clip.
- **Rect operands come back.** A failing real `rectfill` keeps `x y w h`
  (gs: `77 1 2 (a) 4 rectfill` under `stopped` leaves `77 1 2 (a) 4`), so the
  decorated one does too. The show family drops its operands instead; that
  was #1179's call and is unchanged.
- A warp that leaves its own `gsave` open and then fails still leaves that
  one level; only the decorator's gsave is balanced.
- **Rect operands are checked first.** `wp--rect-ok` tests for four numbers
  on top of the stack before `4 array astore` (pscat's `astore` empties the
  stack on a stackunderflow). Otherwise the real operator runs and raises its
  own error, or draws the array form unwarped. `wp--rect-ok` uses `//false`
  because pscat reports the last executed *name* as the offending command.
- Known gaps: guards unwind to the nearest mark and closure warps leave their
  scratch dictionary open (#1199); pscat drops operands on errors (#1198).
