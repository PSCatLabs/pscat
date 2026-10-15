# #451: a better paper clip

Follows #214 (`notes/214-released-look.md`). `rel-page /Clip` drew each clip as
three nested ellipses with a shadow: it read as a coin, not as bent wire.
Before and after (the same memo, seed and sheet; original, one copy, three
copies): `notes/451-clip-before.png`, `notes/451-clip-after.png`, from
`docs/dev/design/typewriter/clip_451.ps`.

## References looked at first

Photos of gem clips (Wikimedia Commons: a clip on a desk, a clip pressed into a
film of water, loose clips in a heap) and of a rusty wire clip on a dark ground.
What they settle:

- A gem clip is one wire with three turns and four straight runs: a wide
  turn at one end, a narrower turn at the other, and a small inner turn. The
  silhouette is a long rounded loop with a narrower loop tucked inside it, offset
  to one side; two free ends, both inside. It is not concentric.
- On a clipped sheet the paper sits between the two loops. From the front one
  loop is wire on the page and the other is behind it, so the page shows only its
  impression. The bend that joins them is at the sheet's edge.
- Rust on paper is the wire's own outline in brown, darkest at the turns and
  where the wire pressed. It is not a blob.

## What shipped (`lib/released.ps` only)

- `rlClipF` (outer loop), `rlClipB` (inner loop), `rlClipW` (the whole wire) build the
  path with `arc` and `arcn`, in a frame whose origin is the sheet's top edge.
  `rlClips` hangs each clip from the edge (it was 28pt down, floating on the page).
  The first clip keeps its x and its 8 degree tilt; later ones tilt by another 11
  degrees each, three in turn, so a row of them is not a row of copies.
- Layers, bottom to top: the rust ghost (the whole wire in a brown tone, heavier along
  the inner loop and in two small blots at the bottom turn and the apex; strength
  0.16 + 0.3 /Age); the dent (the inner loop as a groove on the shadow side and a
  pale lip on the light side); the outer loop's shadow; the wire (dark edge, mid
  grey, glint).
- Everything goes through `rlSet`/`rlToneTo`/`rlToneX`, so a copy greys the clip and the
  copier's curve whitens the dent and the rust (the dent is still faintly there
  at generation 1, mostly gone at 3) while the wire stays dark.
- No random numbers, so the library's generator stream and every other mark are
  untouched. No new option keys. `/Shadow` still sets the shadow and dent
  strength. No tracked render uses `/Clip`, so `scripts/identity_check.sh` lists
  no file as changed.
- `tests/released.rs`: `a_clip_is_a_gem_wire_at_the_top_edge_with_the_inner_loop_only_a_dent`.

## Not done

- The clip is not softened by a copy (`/Soften`, #450 covers stamps, notes and
  bars). Under a copy the wire stays crisp, as the copier's lid image of a real
  clip is.
- The clip's bend is drawn at the sheet's edge, inside the sheet: marks are clipped
  to the sheet, so the part of the wire that wraps over the edge is not shown.
