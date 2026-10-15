## Alpine Lake: plane streaks read through the far mist banks (issue #476, 2026-10-14)

#407 made the far range's mist four banks of haze, and #446 made their ends dissolve sideways.
One leftover from both: where a bank crosses a shadow plane, the bank's outer `pkwet` passes are
opaque and nearly `lkFAR`, so the plane's streaks stopped dead at a horizontal line and the bank
read as a flat light bar over the plane. This change lets the streaks run on through the bank. All
looks were judged at 2x against a fresh render of `main`, which matched the committed still byte
for byte.

**What changed.** After the two pkwets, `lkmist` calls `lkghosts`, which lays the far range's
shadow planes again over the banks:

- `lkrange` now notes each far-range plane in `lkPLANES`: its `lkLOG` slot (the random state its
  pulls were laid from) and its corners. Noting it draws no random numbers, so nothing re-rolls.
- The plane's triangle and `pktrowelplane` call moved into one proc, `lkplane`, used by `lkrange`
  and by the re-lay, so the two cannot drift apart. The bank outlines became one table, `lkBANKS`,
  read by `lkbanks` (the fill) and by `lkbanksm` (the same lenses grown by m, as a clip path).
- `lkghost m` restores each plane's logged seed and calls `lkplane` again, clipped to the range's
  outline and to the banks grown by m. The pulls fall exactly where the plane's did, and the gaps
  between them show the mist.
- `lkghosts` runs four of these, each over the last, as checkpoints 22 to 25: m = 9, 6, 3, 0,
  in the plane's colour mixed 0.3, 0.55, 0.75 and 0.9 toward the mist core `lkMC`. Just outside a
  bank the streaks are close to the plane's own. In the core they are a faint ghost. Using four
  steps instead of one keeps the veil from having an edge.

**Checks.**
- With the re-lay switched off, the refactor (`lkplane`, `lkBANKS`) renders byte-identically to
  `main`.
- With the mist removed, a plane-coloured re-lay over a large clip puts every streak at the same
  x as the plane under it. The only pixel differences are anti-aliasing. Drawing identical marks
  twice compounds their edge coverage and fills some sub-pixel gaps, which a wrong seed would not
  explain.
- No `setalpha` is used, so Ghostscript (`gs -dNOSAFER`) renders it with no fallback (exit 0, no
  output). Its render shows the banks and the streaks through them.

**Variants tried at 2x** (in order):
- Three layers (m = 7, 3, 0 at 0.3, 0.55, 0.8): the streaks came through, but the outermost layer
  read lighter than the haze it sat on, which left an edge.
- Four layers at 0.15, 0.4, 0.6, 0.8: the streaks ran through cleanly. Over a plane the bank broke
  into a darker stretch, though, so bank 1 read as two pieces, and the plane's diagonal lower edge
  cut crisply through the haze.
- Four layers at 0.3, 0.55, 0.75, 0.9 (shipped): each bank still reads as one band across the
  plane, and the streaks show through it, fading toward the core.

**Containment.** `lkmist` reseeds itself, and the re-lay restores each plane's own seed, so only
the band moves. A row diff of the old and new 2x stills shows that only rows 422 to 484 changed,
from column 285 rightward (the band over the planes). Every other row is identical, including the
reflection.

## Not done

- **The reflection.** Parts 22 to 25 are not pulled into the lake. The reflected band is blurred
  and sits mostly under the foreground bank, so the old light bar there is hard to see. Pulling it
  would cost four more full-scene replays. Filed as
  https://github.com/jeromebanks/postscript_interpreter/issues/508.
- The manifest's technique text still holds, so it is unchanged. The piece's header now lists the
  re-lay under `pktrowelplane`.
- HANDOFF.md has no entry for this leftover, so it is unchanged.
