# #459: a bound wrapper's grestore inside a dkbatch, a documented limit (2026-10-14)

## Decision

drykit cannot notice the `grestore` of a procedure bound before a
`dkbatch`. Standard PostScript has no exact way to do it, so #459 closes as
a docs fix: the limit, why it holds, and three workarounds are now spelled
out. A pscat-only exact route is filed as #488.

## The limit

#444 keeps a plain `clip` made inside a batch by putting shadows of `clip`,
`eoclip`, `rectclip`, `initclip`, `setgstate`, `initgraphics`, `gsave`,
`grestore` and `grestoreall` into userdict while the batch's procedure
runs (`notes/444-plain-clip-in-a-batch.md`). `bind` replaces a name with
the operator it finds, so a procedure bound before the batch calls the
operators directly and the shadows never see it. Three cases follow:

- **Bound code clips and strokes.** drykit sees neither the clip nor the
  restore. The strokes keep whatever drykit last tracked (the `dkclip`
  region, the last plain clip captured, or none), as before #444.
- **Unbound code clips, bound code restores.** This is the bad case, and
  the one #459 is about. In `/wrap { gsave exec grestore } bind def`, the
  callback in `{ region clip line } wrap line` is unbound, so its `clip`
  goes through a shadow and its stroke captures a gstate. The wrapper's
  `grestore` is unseen, so the later `line` is drawn under the stale
  capture. The leak lasts until drykit sees the next clip change: a
  shadowed `clip`, `grestore`, `initclip` and so on. Measured: 2,488
  pixels differ from the same program drawn at once (the test row below,
  with the workaround taken out).
- **Unbound code clips, makes no stroke, bound code restores.** Harmless.
  The capture is lazy, so the next stroke after a clip change takes it,
  and drykit has nothing stale to hand out.

drykit's own media (`dhhatch`, `dhsmudge`, ...) clip through `dkclip` and
are unaffected. No committed render hits the bad case.

## Why standard PostScript cannot see it

- **A gstate snapshot per stroke.** `gstate` makes a new object, and `eq`
  compares gstates by identity, so a snapshot taken at each `dkstroke`
  never equals the capture. Comparing two gstates' clips needs an
  operator PostScript doesn't have.
- **`clippath` per stroke.** The only standard way to read the clip
  back. pscat's `clippath` under a nested clip is a mask trace,
  about 1.6 ms per stroke at 144 dpi, roughly a whole `dkstroke`, which is
  what #312 avoided. It is still not exact: two clips can share a
  `clippath pathbbox`, and the traced path is not the chain.
- **Sentinel graphics-state parameters.** Set a parameter at capture time
  that `grestore` would put back (flatness, a transfer procedure, ...).
  Rejected: the caller can read them back, and most of them change ink.
- **gsave depth.** `grestore` changes it, but PostScript gives no way to
  read it.
- **An extra gsave level in the shadow `clip`.** The bound `grestore`
  would then pop drykit's level instead of the wrapper's, and the clip
  would stay. That changes the program's meaning.

An exact route needs a non-standard operator: in pscat, a test of whether
the clip in force is the same chain as a gstate's. The captured gstate
keeps that chain alive, so an identity test cannot be fooled by reuse.
That is #488. gs would keep the current behaviour, so the two would draw
such a program differently, and it adds a public operator for one edge
case. So it is left as a follow-up, not done here.

## Workarounds (each a test row)

- `dkclip` inside the callback: `{ region { line } dkclip } wrap`.
- Call the bound wrapper inside a `gsave ... grestore` the batch sees:
  `gsave { region clip line } wrap grestore`. The outer `grestore` goes
  through the shadow and gives back the clip entry from before the
  wrapper.
- Bind the wrapper inside the batch. `bind` only inlines operators, and
  inside the batch `gsave` and `grestore` resolve to drykit's shadows,
  which are procedures, so a wrapper bound there keeps the names.

## Evidence

- `tests/drykit.rs` `a_plain_clip_in_a_batch_renders_as_at_once` gains two
  rows: a bound wrapper called inside a seen `gsave ... grestore`, and a
  wrapper bound inside the batch. Each is byte-identical in a batch to
  the same program drawn at once, like the existing `dkclip` row. The leak
  itself (the bare `{ RING CL } dktwrap FREE`) differs by 2,488 pixels.
  It was run as a trial row and left out, so a test does not pin a wrong
  render as expected.
- Ghostscript (correctness only, png16m at 144 dpi, the same program with
  artkit, paintkit and drykit prepended): the seen-gsave workaround is
  byte-identical as a batch and at once, and the bare leak differs. gs
  agrees with pscat on both.
- Identity: see the PR. Only comments, docs and a test changed. lib/drykit.ps
  changes only in comments and the `dkbatch` `@summary`.
