## pkpull takes /Scatter, an across-spine offset for each start (issue #346, 2026-10-14)

**Gap.** `pkpull` started every stroke exactly on the spine; `/Wander` slides
a start only *along* it. A fall pulled down off a lip, or grass pushed up from
a ground line, therefore began on one ruled line and read as a picket fence
hung from a rail (found while painting `gallery/evening_stream.ps`, #187).

**Change.** `<< /Scatter pts >> pkpull` offsets each stroke's start by a
random amount in [-pts, +pts] along the *normal of the spine* at the sample
the start slid to (the stop's tangent angle, rotated 90 degrees). It is the
same key `pktap` has (#141: one key, one meaning: a random offset of each
mark from the spine, in points, >= 0). The offset is applied before the Bow
control points are built, so a bowed stroke moves with its start.

**Decisions.**
- *Across, not a box.* `pktap`'s offset is a box in user-space x and y.
  `pkpull` already has `/Wander` for along-the-spine, so `/Scatter` is the
  other axis only: on a horizontal lip it is vertical, on a curve it follows
  the curve's normal (so it is not the same as "along the pull" once the
  spine is not perpendicular to `/Direction`).
- *Unset draws nothing.* There is no default. Unset, the code path is the old
  one and draws the same four values per slot, so every existing render is
  byte-identical, random stream included (`scripts/identity_check.sh --loader
  lib/paintkit.ps`). Set to anything, even 0, it draws one more value per
  slot (the fifth, after the existing four), so `/Scatter 2` and `/Scatter 6`
  place the same strokes and only the offset size changes; `/Scatter 0` is
  not the same stream as unset. Gating on the key being present, not on its
  value, keeps "a fixed number of draws per slot" true for a given option set.
- *Validation.* A procedure or a negative value is refused by the usual named
  guards (`pkpull-scatter-must-not-be-a-procedure`,
  `pkpull-scatter-must-be-non-negative`), with selftests.

**Cookbook.** Recipe 11 (Grass) uses `/Scatter 3` on the rim, its picket-fence
failure entry now names `/Scatter`, and the #346 row is gone from "Tools not
yet landed". Only `grass.png` changed. The tufts layer stays: it is a separate
dimmer pass, not a workaround.

**For PR #348 (Evening Stream).** The fall there can now add `/Scatter 4`..`6`
to its `pkpull` options (`gallery/evening_stream.ps`, `evfall`); that changes
its render, so it is left to that PR.

**Deferred.** Nothing.
