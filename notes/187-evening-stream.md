## Evening Stream: the second wet-on-wet landscape (issue #187, 2026-10-14)

Epic #112's second landscape, `gallery/evening_stream.ps` (760x560, seed 187),
painted after the round-2 tools landed: directional `pkwet` (#181), loaded
colours (#183, L1-L5), the brush motions (#184) and the canvas surface (#182,
S0-S4). No `lib/` change and no landscape helper. Committed 2x still at
`gallery/renders/evening_stream.png`, a site card, a `show.sh` entry, a gallery
README row and a README paragraph.

- **Scene.** An autumn evening: a stream comes out of the glow where the sun
  went down, bends left over a little fall, and widens between near-black banks.
  There is a backlit far wood and a low misted ridge, a cabin with its lamp lit,
  autumn trees and evergreens framing it, and grass tips catching the light. It
  differs from Alpine Lake in sky, season, water and foreground, as the issue
  asked. Before painting I looked at real wet-on-wet sunsets (Bob Ross's *Home
  Before Nightfall*, *Golden Sunset*, *Sunset Aglow*, *Evening Waterfall* and
  *Around the Bend*). From them: the glow is the only near-white, the foreground
  is near black, backlit trees are dark masses with warm rims, and clouds are
  dark bodies lit along one edge.
- **How each of #118's gaps is answered.**
  - *Value range.* The banks are `[0.05 0.06 0.03]`. The only near-whites are
    the glow, its reflection in the upper reach, the fall and the lamp.
  - *Pulled reflections.* The far wood paints only in loaded colours (`pkdab`
    and `pkfan` `/Colors`) and in the colour it inherits (its mass fill). So
    the stream pulls the whole element down in one directional `pkwet`
    (`/Direction 270`, mirror and `srand` inside the procedure), and
    Alpine Lake's part-by-part `lkdo` record and replay is not needed. This
    exercises #183's `puwet` hook in a composition. Only the upper reach, the
    stretch near the far shore, gets the wood's reflection; the nearer reach
    reflects the sky gradient only (#118's review lesson: a mirror about
    one waterline is wrong on nearer bends).
  - *Double-loaded marks.*
    - The sky's criss-cross `pkbroad` carries the local sky 2.5% lighter and
      2.5% darker at `/ColorMix 1`.
    - The pines carry `[lit dark dark dark]` at `/ColorMix 0.15` with `/Light`
      toward the sun.
    - The knife carries a lit and a dark edge (`/Light` on the cabin walls,
      roof, fall lip and rocks).
    - The autumn trees carry five leaf colours on `pkdab`, with a warm
      `/HighlightColor` and a violet `/ShadowColor`.
    - The light lines on the water are `pkbroad` loaded with glow and water.
  - *Motions.* `pkcrisscross` for the sky, `pkscumblein` for the clouds,
    `pkscumble` for the foam, `pktap` for the wood's crest and the meadow's far
    edge, `pkpull` for the fall and the grass, `pkzigzag` for the water lines.
  - *Clouds with form.* Three attempts. First, spray bars like Alpine Lake's,
    which had no form. Then loaded `pkdab` clumps lit toward the sun, which
    read as leopard spots at cloud scale. Then `pkscumblein` tiny circles
    painted with `pkbroad`: the brush must be at least twice the circle radius,
    or each circle reads as a ring (a "Cheerio" cloud). The final cloud scrubs
    the whole region in the lit colour, then scrubs the body over the same
    region shifted up 0.3 of its height and unclipped, so a crescent of light
    is left along the lumpy underside, all inside one `pkwet /Soft 0.6`. Two
    things failed along the way: clipping the body to the cloud let the lit
    layer's `pkwet` halo ring the top as well, and a `pkscumble` chain along
    the underside read as a sausage. This was a workaround, filed as #345 and replaced in #384.
  - *Canvas.* `pkcanvas` is called once, first, in page space. `/Touch` is on
    the water lines (0.7), foam (0.8, 0.65), meadow strokes (0.85) and grass
    (0.7, 0.8).
- **Not used.** The pigment palette (#185 has not landed), so colours are mixed
  with artkit's `mix3` and `shade`. `/TipColor` (#229) would have given the
  grass a dark stem and a lit tip. `pkdry /Colors` (#230) would have let the
  foam carry a shadow.
- **Gaps found, filed as children of #112.**
  - #345: light-aware massing (a lit side on a scumbled region) for clouds
    and bushes.
  - #346: `pkpull` has no across-spine `/Scatter`, so the fall reads as a
    picket fence hung from a rail.
  - #347: a brushed edge for filled masses. Banks, meadows and stream edges
    are ruled vector edges, hidden here by hand with tapped dabs, bushes and
    grass.
  - Already open and confirmed by this piece: #179 (`pkwet /Under` as a
    position-dependent backdrop; each cloud's `/Under` is one sky colour over
    a graded sky) and #180 (knife plane mode; the cabin walls are a hand loop
    of clipped vertical pulls).
- **Reproducibility.** Two 2x renders are byte-identical (`cmp`). `--lint` is
  clean. Ghostscript renders the whole scene without errors (no `setalpha`);
  only its rand stream differs, so the marks fall differently.
- **Render time.** pscat 1x about 13.4 s, the 2x still about 14.2 s (release
  build, one run each after the gate). Ghostscript 10.07.1 at 72 dpi about 10.2 s
  (`gs -q -dNOSAFER -dNOPAUSE -dBATCH -sDEVICE=png16m -r72 -g760x560`).
