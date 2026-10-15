## Alpine Lake: pulling the plane-streak re-lay into the reflection is not worth it (issue #508, 2026-10-14)

#476 re-laid the far range's shadow planes over the mist banks (`lkghosts`, checkpoints 22 to 25)
so their streaks read through. The lake's reflection replays the scene but not those four parts, so
the reflected band still crosses a reflected plane as the old light bar. #508 asked to pull them
into the lake. Measured at 2x against a fresh render of `main` (which matched the committed still
byte for byte), the change is invisible, so nothing was changed in the render.

**What was tried.** Each variant added pulls straight after the part-21 pull of the band
(`lkMC lkFAR 21 10 6 lkpull`), the same `lkpull` the rest of the reflection uses:

| variant | pull(s) | pixels changed (of 1520x1120) | largest channel change |
|---|---|---|---|
| core only, under the mist core | part 25, plane colour mixed 0.9 toward `lkMC` | 86 | 45 of 255 |
| core only, under `lkFAR` | part 25, same colour | 53 | 11 |
| one layer | part 24, mixed 0.75 | 243 | 44 |
| all four layers, under `lkMC` | parts 22 to 25, mixed 0.3 / 0.55 / 0.75 / 0.9 | 393 | 45 |
| all four layers, under `lkFAR` | same | 293 | 11 |
| all four, replayed sharp, not pulled | `22 lksharp` to `25 lksharp` (the strongest case: no blur at all) | 111 | 18 |

Every variant changed pixels only inside cols 745 to 1165, rows 981 to 1050 (the reflected band
over the planes, right of the foreground bank's crest), so the containment held: the pulls reseed
their own wet, nothing outside the reflected band moved. Side by side at 2x and 3x zoom the base
and the all-four variant cannot be told apart.

**Why it is invisible.** The reflected band is thin (the banks are 4 to 10pt thick), the
foreground bank hides most of the stretch where it crosses the planes, and what is left is blurred by the 6-layer pull and then crossed by the horizontal
`pkbroad` water pulls and ripples. Even the unblurred replay moves 111 pixels by a few levels.

**Cost route, if it is ever wanted.** The four layers did not need four full-scene replays:
`lkghost` reseeds each plane from `lkPLANES` and `lkLOG` and clips itself, and `lkmy` is already set
by the lake-time replay of `lkmist`, so a pull whose proc is `{ gsave lkmirror 0 lkghost grestore }`
lays exactly part 25's marks without walking `lkscene`. Not built, since there is nothing to see.

**Recommendation.** Close #508 as not planned. The render is unchanged (the `.ps` differs from
`main` only in a comment).
