# #206 — Alpine Lake visual polish

Optional art polish on `gallery/alpine_lake.ps` after #191. All changes
were judged by eye at 2x against a fresh render of `origin/main`'s file,
which matched the committed still byte for byte. The issue's acceptance
criteria ask for a NOTES.md entry; per repo convention it lives here.

## What changed

- **Mist at the near range's foot (part 35).** The old lift carried two
  curves of 1pt spray at `/Density 90` through 14 layers, which at 1x and
  2x read as a coarse speckled band, gravel more than haze. The 16pt
  rectfill under it sat outside the lift. Now the band rides the lift
  itself, as a filled shape with a wavering top, so the 14 graded copies
  make a soft gradient whose steps follow the wave instead of ruling. The
  spray is a fine grain on it: `/Speck 0.6 /Density 50`. Variants tried
  at 2x: Speck 0.6/Density 90 with the old band (finer but still a
  speckled band), Speck 0.7/Density 170/Nozzle 8 (more fused, still a
  noisy bar), and the band in the lift with Density 50 or 120 (both read
  as haze; 50 is cheaper and less grainy). The reflection's `35 lksharp`
  replays the same, so the reflected band improved too.
- **Reflected snow.** The four snow parts (13, 14, 33, 34) are pulled in
  `lkSNOWR`, the snow mixed 0.4 toward the water before `lkpull`'s own
  0.15, since a reflection is darker and flatter than what it mirrors.
  The near range's snow is pulled deeper (`/Offset 16 /Layers 9`, still
  2pt a layer) and with `/Stretch 2` (`lkPST`, default 5) for more
  sideways wobble. On their own these barely softened the blocky peaks
  at the foot of the lake, because the core pass stays crisp. What did
  it: ripples across the bottom 80pt of water, three short `pkbroad`
  strokes a row every 3pt, in the reflected range's own colour, so they
  disappear on the range and cut the snow into broken bands.
- **Ruled light lines.** Most of the tan "rules" were the long (200 to
  360pt) `pkbroad` grazes, warm because the water near the shore mirrors
  the dawn horizon, running out to one or two striations. Grazes are now
  three short strokes a row (30 to 130pt), width 3 to 7, `/Edge 0.45`,
  mixed toward the reflected wood (fading in over the 40pt under the
  shore) or 0.6 toward the reflected range below that. The 14 liner
  light lines are 16 runs of 1 to 3 glints each, of random length (4 to
  26pt), weight (0.6 to 1.3pt) and strength (mixed up to 0.55 toward the
  water under them). The full-width shore line is broken into 40 to 200pt
  lengths with small gaps.

## Timing

1x, `--png --page 760x560`, back to back on the same machine (user time):
before 25.8s / 25.9s, after 26.2s / 26.4s, about +0.4s (+1.5%). The
issue's ceiling was the ~51s it measured on a slower setup. The 2x still
takes about 27s either way. Ghostscript (`gs -dNOSAFER`) still renders
it without error.

## Not done

- The far range's mist band (element 2, `lkmist`) is still 1pt spray at
  2x; it reads as granular vapour, which the cookbook calls correct, but
  could get the same treatment.
- Shadow planes (#361) were deliberately left alone.
