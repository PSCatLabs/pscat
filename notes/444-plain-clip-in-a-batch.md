# #444: a plain clip inside a dkbatch, kept with gstate objects (2026-10-14)

## Decision

A plain `clip`, `eoclip` or `rectclip` made inside a `dkbatch` is now kept
for the strokes made under it, exactly, nested and even-odd clips included.
The capture is a Level 2 `gstate` (#394), taken **once per clip change**,
not per `dkstroke`, so strokes under one clip share one object and
`dkZsame`'s identity test still groups them into shared catches.

Standard PostScript cannot ask "is the clip the same as last time?", so
drykit watches the changes instead. While a batch's procedure runs,
`dkbatch` puts shadows of `clip`, `eoclip`, `rectclip`, `initclip`,
`setgstate`, `initgraphics`, `gsave`, `grestore` and `grestoreall` into
**userdict** (not a dict pushed on the dict stack: that would catch the
caller's own `def`s inside the batch and lose them when it closed). They
are taken back when the procedure returns, on an error too.

## What shipped (lib/drykit.ps)

- The batch dict gains `/pc` (the plain clip in force: `null` for none,
  `/dirty` for changed but not captured, or the gstate captured), `/pcs` (a
  `[rest pc]` stack pushed by `gsave`, popped by `grestore`) and `/g` (a
  gstate of the state the batch opened in).
- The clip shadows run the operator, then set `/pc` to `/dirty`.
  `grestore` pops the entry saved by its `gsave`; with nothing to pop
  (a `grestore` past the batch's own level), or after `grestoreall`, it
  goes to `/dirty`. Every transition the shadows cannot follow goes to
  `/dirty`, never to `null`: `/dirty` only costs grouping, `null` with a
  changed clip would draw the stroke under the wrong clip.
- `dkstroke`, in a batch: `/pc` null keeps the #312 rule (the `dkclip`
  entry, or none). Otherwise a `/dirty` `/pc` becomes `gstate` (the one
  capture), and the record's `/clip` is that gstate.
- `dkZclipat` (render): a gstate `/clip` is put back with `setgstate
  newpath`, a dkclip entry goes through `dkZclipin` as before. `dkZdraw`
  now sets the colour after the clip (identical for the old entries,
  needed since `setgstate` brings the stroke-time colour back).
  `setgstate` replaces the clip in force rather than intersecting it, so
  the batch's own clip is not applied twice.
- A batch that ends with `/pc` non-null (a clip made at the batch's own
  level and never given back) renders inside `gsave /g setgstate`, so the
  strokes made before that clip are not cut by it.
- `dkclip` inside a batch under a plain clip marks `/dirty` for its body
  (its region changes the clip a stroke must capture) and gives the plain
  entry back after.
- `save`/`restore` need nothing: the batch dict lives in local VM, so a
  `restore` rolls `/pc` and `/pcs` back with the graphics state.
- The shadows install only if every one of the nine names resolves to
  systemdict. A caller with its own `gsave` (say) in userdict keeps the
  old rule rather than a half-tracked stack.

Limit: a procedure bound before the batch has `clip` resolved to the
operator, so its clips are not seen; its strokes use the clip in force
when the batch ends, which is the old behaviour. drykit's own media
(`dhhatch` etc.) use `dkclip` and are unaffected. pscat has no
`clipsave`/`cliprestore`; gs does, and they are not shadowed, so in gs a
`cliprestore` after a stroke captured an inner clip leaves later strokes
under that inner clip (#454).

Behaviour change: a batch that ends under a clip it made used to render
every stroke under that clip (the documented rule); now the strokes made
before the clip stay whole, as they would drawn at once. No committed
render relies on the old rule.

No specimen: nothing renders differently unless a caller clips inside a
batch (the same call as #393).

## Evidence

- `tests/drykit.rs` `a_plain_clip_in_a_batch_renders_as_at_once`: ten
  cases, each byte-identical in a batch to the same program drawn at once:
  an `eoclip` ring nested in a rotated square, `rectclip` then `initclip`,
  a clip the batch ends under (the earlier stroke stays whole), a plain
  clip inside a `dkclip`, a `dkclip` inside a plain clip, `setgstate`,
  `save`/`restore` (also identical on main: a regression guard), an
  `/After` stroke under a plain clip, `initgraphics` and `grestoreall`.
  Against main's drykit the first eight cases other than `save`/`restore`
  differ by 2,488 to 7,813 pixels. Control in the test: the
  shadows blocked by a userdict `gsave` lose the ring.
- `plain_clips_in_a_batch_group_as_dkclip_regions_do`: two overlapping
  hatched regions under `gsave ... clip ... grestore` in one batch are
  byte-identical to the same regions as `dkclip` calls, and take the same
  number of catches (6 and 6). A fresh gstate per stroke takes 96: the
  shared catches don't show in the pixels (a catch shared or not lands the
  same ink), so the test counts `dkcatch` calls instead.
- `dkbatch-keeps-a-plain-clip` self-test: two strokes under one clip share
  one gstate, a stroke after the `grestore` has none, a new clip takes a
  new one, the shadows are gone after a batch and after a failed one, and
  a userdict `gsave` turns them off. Against main's drykit it fails.
- Ghostscript (correctness only): `ghostscript_keeps_a_plain_clip_in_a_batch`
  (a middle-third clip keeps 0.33 of the ink in pscat and 0.33 in gs;
  main's drykit keeps all of it in both). The nested-ring case renders
  byte-identical in gs as a batch and at once (png16m, 144 dpi); main's
  drykit differs there. All nine names resolve to gs's systemdict as
  operators (so `bind` inlines them and no shadow calls itself); a gs
  batch running `rectclip`, `initclip`, `setgstate`, `initgraphics` and
  `grestoreall` leaves no shadow behind, and a second batch still clips.
- Identity: see the PR (`scripts/identity_check.sh --loader` for drykit
  and the media libraries, `--files` for the prototypes). No committed
  render makes a plain clip inside a batch, so nothing changes.
- Cost: one `gstate` per batch (the state it opened in), a few `where`
  lookups and userdict puts per batch, and one key lookup per stroke. The
  budget tests run in the gate as before; no timing was taken.
