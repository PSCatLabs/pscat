## drykit: a /From cut at a touch band change joins without a ring (issue #1444, 2026-11-27)

Two adjacent pieces (`/To c` then `/From c` on one canvas) showed a faint arc
where the stroke's touch changed band at or just after the cut: `/Width 8
/Touch { 0.4 mul 0.3 add } /Seed 3`, a line from 10 to 90 at 432 dpi, cut at
0.47 or 0.50, worst 82 levels, 86 to 90 pixels over 32, darker, 26 pixels
behind the band change. `/From 0 /To t` (psanim's growing stroke) was never
affected.

**Cause.** `dkSsub` closes a lane's run where its touch band changes and starts
the next run at the same stop (the same array object). That run starts round,
and the run before it ends round there. `dkrender` draws level-major (every
faint level, then every bright one), so in the whole stroke the new band's
faint level and halo lie under the old band's bright level near that stop.
In two pieces the second one painted all its levels over the first one's
finished ink. `dkZbehind` (#1419) clips only a run that the cut falls inside,
so it did not help here. The run that starts at or just after the cut keeps its
round start. A clip of it behind the cut was tried and dropped: it trades the
ring for a lighter half-disc (the new band's catch has tooth the old one's
lacks; 296 pixels over 32 at cut 0.50).

**Fix.** The second piece draws what the whole stroke draws over the new
band's start. `dkFrun` (`dkFover`) finds a run in the part that carries on
from the run before it, where the stop they share lies within one reach
(half-width, halo and a device pixel) of the cut. It puts the earlier run's
tail (three reaches back) before it as an *overlap run*, with flag 16 on its
stops and `[f hm cl n cx cy chw dx dy bx by br D side]`. `dkZink` draws the
overlap only at the levels the whole stroke draws after the new band's
faintest: all but the faintest, or all of them when the new band's group comes
first in each pass, because the stroke has drawn that band before (`dkFkeys`,
the order in which `dkrender` meets the whole record's keys). `dkZover` clips
it to the shared stop's reach on the far side of the cut: behind the cut when
the stop is after it, under the new run's round start; ahead of the cut when
the stop is before it, over the old run's round end from the first piece;
both at the cut. So near the stop each pixel's top level is the whole
stroke's.

**Measured** (whole against halves, worst level / pixels over 32):

| case (432 dpi) | before | after |
|---|---|---|
| rising touch, cut 0.47 / 0.50 | 82/86, 82/90 | 26/0, 26/0 |
| falling touch, cut 0.47 / 0.50 / 0.53 | 82/186, 82/448, 82/45 | 31/0, 31/0, 31/0 |
| touch back to a drawn band, cut 0.25 / 0.73 / 0.75 | 82/64, 82/160, 82/170 | 32/0, 29/0, 29/0 |
| `/Levels 3`, cut 0.47 / 0.50 | 110/152, 110/165 | 22/0, 22/0 |
| curve `/Width 6`, cut 0.47 / 0.50 | 62/5, 82/77 | 28/0, 29/0 |
| a cut away from any band change (0.25, 0.40, 0.75) | 31 to 32/0 | the same |

At 144 dpi the same cases go from 54 to 77 levels (6 to 25 pixels over 32) to
27 to 32 (none). A constant touch seams at 12 to 21. drychalk on the #1420
fixture (`/Tilt 75 /Side 8 /Seed 1`, 432 dpi): cut 0.3 goes from 179 pixels
over 32 to 42. The remaining `/Tilt` residue (worst 71 at most cuts, before and
after) is #1420's: the lanes overlap and are all cut at once.

**The second lead in the issue is not a seam.** The lead was `/Width 8 /Touch 0.5
/Pressure { 0.3 mul 0.85 add }` halves, worst 41 at 432 dpi. At 144 dpi it is
worst 39 on 3 pixels. Those pixels are at the stroke's far end, whatever the
cut, and `/From 1e-9` alone shows the same: drawing in part mode at all differs
from the whole stroke at the end of a run whose width varies. Raising
`dkbudget` leaves it unchanged, so it is not #1419's chunking. It was there
before this change, and it is proposed as a follow-up.

**Unchanged.** Whole strokes do not reach this code. The overlap is made only in
part mode, so `scripts/identity_check.sh` is byte-identical for drykit,
drychalk, drycharcoal and drycrayon. Keys, defaults and errors are unchanged.
Two band changes within one reach are each matched against the run just
before them, not layered three ways. In a `dkbatch` the draw order comes from the
stroke's own keys, not the batch's.
