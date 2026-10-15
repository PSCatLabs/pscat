# Gallery

Generative art written in plain PostScript for `pscat`. Each piece is one
`.ps` file in this directory with a rendered still in `renders/`; together
they are the most demanding tests the interpreter has. The published
[gallery](https://jeromebanks.github.io/postscript_interpreter/gallery.html)
shows them by category, with a page for each piece.

**The manifest.** [`manifest.json`](manifest.json) is the one list of what
the gallery shows. The site's gallery and its per-piece pages, the highlights
grid in the top-level README, the table below and `./gallery/show.sh` are all
generated from it. Nobody edits `gallery.html` by hand.
Every picture there has a **View source** link that opens its whole program in
a scrollable window (the site build stages each entry's `source`), so a new
entry gets one with nothing more to do.

## Adding a piece

1. Write `gallery/<name>.ps`, with a `%%BoundingBox` header that gives the page size.
2. Render the still to `gallery/renders/<name>.png` (see "Re-rendering the
   stills" below).
3. Add an entry to `manifest.json`: title, category, the libraries the file
   loads, a one-sentence `blurb`, a few sentences on the technique, the `alt`
   text for the picture, the page size, the slideshow `speed` and the date.
   Copy a neighbouring entry. Set `featured` only for the handful shown in the
   README, and `playground` only if the program reads no other file (the
   browser build has no filesystem).
4. Run `python3 scripts/gallery.py sync` to refresh the generated tables, then
   `python3 scripts/gallery.py check`. CI runs the check on every pull
   request: it fails if a tracked `gallery/*.ps` has no entry, an entry's
   render is missing, or a generated part is out of date.

The check also keeps the wording newcomer-friendly: describe what the piece
does, not which project stage or issue produced it.

## The pieces

<!-- gallery:table:start -->
### Geometry and fractals

Pictures that fall out of a formula: roses, spirals, tilings and fractals.

| Piece | File | How it is made |
|---|---|---|
| [Golden Bloom](https://jeromebanks.github.io/postscript_interpreter/gallery-golden-bloom.html) | [`golden_bloom.ps`](golden_bloom.ps) | Each floret sits at the golden angle (about 137.5 degrees) from the last, at a distance that grows with the square root of its number. That one rule packs the disc evenly and produces the interlocking spirals found in sunflowers and pine cones. |
| [Cathedral Rose](https://jeromebanks.github.io/postscript_interpreter/gallery-cathedral-rose.html) | [`cathedral_rose.ps`](cathedral_rose.ps) | A Maurer rose joins points on the rose curve r = sin 6θ with straight lines, stepping a fixed number of degrees each time. Two walks, one in 71 degree strides and one in 97, are layered; the straight lines alone weave the lace. |
| [Fern, After Barnsley](https://jeromebanks.github.io/postscript_interpreter/gallery-fern.html) | [`fern.ps`](fern.ps) | Michael Barnsley's fern is built by picking one of four simple transformations at random, over and over, and plotting where the point lands. After 48,000 plots a frond appears, though no line of it was drawn. Change the seed and the same fern grows from different dots. |
| [Frost Mandala](https://jeromebanks.github.io/postscript_interpreter/gallery-frost-mandala.html) | [`frost_mandala.ps`](frost_mandala.ps) | One circle spawns six smaller circles around its rim, each of which does the same, and every generation is twisted by 11 degrees. The result is a snowflake-like mandala built from more than fifteen hundred circles. |
| [Woven Labyrinth](https://jeromebanks.github.io/postscript_interpreter/gallery-woven-labyrinth.html) | [`woven_labyrinth.ps`](woven_labyrinth.ps) | Sébastien Truchet's 1704 tile is a square split corner to corner into two contrasting triangles. Each of 256 copies is turned by a random quarter turn, and because the diagonals meet from tile to tile, the single motif chains into a continuous, flowing labyrinth. The program carries its own copy of artkit's `truchet`. |
| [Infinite Descent](https://jeromebanks.github.io/postscript_interpreter/gallery-infinite-descent.html) | [`infinite_descent.ps`](infinite_descent.ps) | A single heptagon is reflected across each of its edges, and the copies across theirs, for four generations. The tiles are drawn on the Poincaré disc, where the hyperbolic plane's infinite tiling shrinks toward the rim; 232 tiles are coloured in rings by generation. It uses artkit's `httile`. |
| [Recursive Peaks](https://jeromebanks.github.io/postscript_interpreter/gallery-recursive-peaks.html) | [`recursive_peaks.ps`](recursive_peaks.ps) | The peaks are triangles subdivided with `gasket` and shaded by altitude, the sparse starfield comes from `carpet`, and the snowflakes overhead are `edgepoly` Koch and quadratic Koch curves. Everything is a fractal from artkit's fractals section. |

### Generative and L-systems

Rules and random numbers that grow into plants, fabrics and fields.

| Piece | File | How it is made |
|---|---|---|
| [Ember Tree](https://jeromebanks.github.io/postscript_interpreter/gallery-ember-tree.html) | [`ember_tree.ps`](ember_tree.ps) | Every branch splits into shorter ones, with the angle and length of each drawn from a small random-number generator the piece carries itself (a linear congruential generator, so the picture is the same every time). The sky is a stack of colour bands and the sun a disc with a layered glow. |
| [Silk Waves](https://jeromebanks.github.io/postscript_interpreter/gallery-silk-waves.html) | [`silk_waves.ps`](silk_waves.ps) | Each thread is one long curve. Two sine waves with different wavelengths push every thread sideways by a different amount along its length, and where the waves reinforce each other the threads gather into silk-like folds. |
| [Hortus Machinalis](https://jeromebanks.github.io/postscript_interpreter/gallery-hortus.html) | [`hortus.ps`](hortus.ps) | Three plants are grown from short rewriting rules (L-systems) that a turtle follows to draw stems. Dried blossoms are then stamped along each plant's own path by walking it with `pathforall` at even spacing, and the labels are set in Palatino, as a plate deserves. |
| [Lodestone](https://jeromebanks.github.io/postscript_interpreter/gallery-lodestone.html) | [`lodestone.ps`](lodestone.ps) | Each filing is traced through a flow field with `advect`. The field comes from `curl2` applied to a hand-composed potential, coherent `noise2` texture plus a radial term, whose perpendicular gradient curls a purely radial field into the concentric loops real filings make around a magnet. |
| [Firefly Census](https://jeromebanks.github.io/postscript_interpreter/gallery-firefly-census.html) | [`firefly_census.ps`](firefly_census.ps) | The star field is a weight procedure (a Milky Way band times coherent `noise2`), the hill's stipple is scattered into the silhouette's own outline captured with `scpath`, the meadow is minimum-spaced grass anchors whose height and tone come from their own height on the page, and the fireflies are two passes over one seed: halos first, then cores, so every core lands inside its own halo. |

### Painting

Brush strokes with bristle, loaded paint, wet edges and paper underneath.

| Piece | File | How it is made |
|---|---|---|
| [Plum Branch in Ink](https://jeromebanks.github.io/postscript_interpreter/gallery-plum-branch.html) | [`plum_branch.ps`](plum_branch.ps) | Every stroke in the picture (trunk, forking branches, grass at the foot, even the background wash) is one centre line walked by artkit's `walkpath` and filled by paintkit's `pkribbon`. A pressure curve narrows each branch to a point, the grass blades taper independently at each end, and jitter gives the loaded-ink edge. |
| [Flying White: Reeds at Dusk](https://jeromebanks.github.io/postscript_interpreter/gallery-flying-white-reeds.html) | [`flying_white_reeds.ps`](flying_white_reeds.ps) | Built around paintkit's `pkdry` dry-bristle brush. Each reed stalk is a loaded base and a drier, more broken tip sharing an endpoint; the water carries a broken reflection and the sky a thin dry-brush mist. The same primitive is used at three scales, over a banded dusk gradient from `mix3`. |
| [Fugitive Pigments](https://jeromebanks.github.io/postscript_interpreter/gallery-fugitive-pigments.html) | [`fugitive_pigments.ps`](fugitive_pigments.ps) | The poem is about impermanence, typeset in Palatino under a title spray-stencilled through its own `charpath` outline. The illustration is all paintkit brushes: the Milky Way is `pkspray` cloud and star dust along two shared curves, the moon wears a spray halo, the reeds are `pknib` tapers with spray-dab seed heads, and the field is `pkdry` broken texture. |
| [Mochi in Denim Blue](https://jeromebanks.github.io/postscript_interpreter/gallery-mochi-denim-blue.html) | [`mochi_denim_blue.ps`](mochi_denim_blue.ps) | A close-cropped study made from a photograph. Broad overlapping `pkoil` masses model the orange-sable dog, `pkdry` breaks the silhouette into a furry edge, and selectively crisp eyes and nose hold the likeness. The result is an interpretation, not a traced copy. |
| [First Rain](https://jeromebanks.github.io/postscript_interpreter/gallery-first-rain.html) | [`first_rain.ps`](first_rain.ps) | paintkit's `pkwash` over a `pkpaper` ground, painted the way a watercolour is: the palest, wettest washes first, then five receding ridges, each darker, drier and tighter than the one behind it. Every colour is composited by the renderer over the paint already down rather than guessed in advance. The rain veil is a wash with its edge pooling turned off, and the reeds are `pkribbon` drawn under the same wash. |
| [Alpine Lake](https://jeromebanks.github.io/postscript_interpreter/gallery-alpine-lake.html) | [`alpine_lake.ps`](alpine_lake.ps) | Painted back to front from paintkit's brushes, with no landscape helper. The sky is laid out with `pkcrisscross` and painted with `pkbroad`; spray clouds and mist are softened into the paint beneath by `pkwet`, and the far range wears banks of haze lifted both ways by a directional `pkwet`; snow comes from `pktrowel` and the shadow planes from `pktrowelplane`, both clipped to each ridge; a far forest is tapped in with `pkdab`; evergreens are `pkfan`; trunks, grass and water lines are `pkliner`. The reflection replays the same self-seeded marks upside down, one single-colour part at a time, each pulled straight down with a directional `pkwet` and then grazed across. |
| [Evening Stream](https://jeromebanks.github.io/postscript_interpreter/gallery-evening-stream.html) | [`evening_stream.ps`](evening_stream.ps) | Painted on one shared canvas (`pkcanvas`) with the newer paintkit tools and no landscape helper. A criss-cross sky comes from a broad brush loaded with two neighbouring sky colours; clouds are scumbled in tiny circles (`pkscumblein`) and lit along their lumpy undersides by the same brush's `/Light`, as from the glow below; a cloud's body is worked wet into the sky with `pkwet /PerFill`, which asks the sky's own colour at each stroke's place, so the body's top edge settles into the darker sky above it; a backlit wood is tapped in with `pktap` and a loaded `pkdab`, its reflection pulled down whole by one directional `pkwet`. Autumn trees take warm highlights and cool shadows, evergreens a fan loaded dark and lit with `/Light`, the rocks and cabin come from a palette knife, a `pkpull` fall and grass finish the bank, and `pkzigzag` light on the water is caught on the canvas weave. The stream's banks, its head, the far shore and the near banks' rims are brushed with `pkbrushedge`, so no big mass keeps a ruled vector edge: the far shore and the banks carry the meadow's own light to the edge (a procedural `/Mass`, lit under the glow and plain toward the viewer), and a near bank cuts the stream back in exactly where the stream runs behind it (a region list for `/Under`). |

### Printmaking and etching

Halftone dots, hatching and relief cuts: pictures made of marks.

| Piece | File | How it is made |
|---|---|---|
| [Out of Register](https://jeromebanks.github.io/postscript_interpreter/gallery-out-of-register.html) | [`out_of_register.ps`](out_of_register.ps) | Every tonal mark is a halftonekit cell, and none is placed by hand. A teal dot plate (sky and water, at 15 degrees), an orange dot plate (the sun's halo and a horizon band, at 75 degrees) and a dark line plate (two ridges shaded by height, at 45 degrees) are each shifted a few units, so the dot lattice moves while the image stays. The flat sun core, the registration crosshairs, the colour bars and the letterpress title are all there too. |
| [Nightfall, Three Cuts](https://jeromebanks.github.io/postscript_interpreter/gallery-nightfall-triptych.html) | [`nightfall_triptych.ps`](nightfall_triptych.ps) | printkit's three presets are used together in one scene rather than as three separate samples. Sky and water are the engraving preset's fine crosshatch at two angles, the mountains are the woodcut preset's grain-following breakup, and the foreground reeds are the linocut preset's bolder, simpler marks. Each silhouette is filled solid ink first, then its own preset runs a second time over the same path in a lighter colour, giving the carved-through-to-the-paper look of gouged wood or lino. |

### Lettering and type

Text set glyph by glyph, flowed into shapes, or written by hand.

| Piece | File | How it is made |
|---|---|---|
| [Ring of Type](https://jeromebanks.github.io/postscript_interpreter/gallery-ring-of-type.html) | [`ring_of_type.ps`](ring_of_type.ps) | Each letter is placed and rotated individually around a circle, using the width of each letter (`stringwidth`) to space it. The ampersand in the middle is drawn from the outline of the character itself (`charpath`), so it can be stroked and filled like any other shape. |
| [Hundred Lines](https://jeromebanks.github.io/postscript_interpreter/gallery-hundred-lines.html) | [`hundred_lines.ps`](hundred_lines.ps) | The HandScript font draws every letter as a fresh handwritten stroke, so no two copies of the same letter are alike. This piece reuses it wholesale and turns two knobs the font leaves alone: the jitter and the pen width are overridden for each line, so the writing gets sloppier from the first line to the last. |
| [The Compositor's Proof](https://jeromebanks.github.io/postscript_interpreter/gallery-compositors-proof.html) | [`compositors_proof.ps`](compositors_proof.ps) | A motto is set inside a round medallion with `tfflow` and a hand-written circle boundary procedure. A body paragraph is justified with `tfblock`, and a two-column colophon set with `tfcols` genuinely spills from the first column into the second. |
| [Released in Part](https://jeromebanks.github.io/postscript_interpreter/gallery-released-memo.html) | [`released_memo.ps`](released_memo.ps) | Typed with `tw-type` on the `/Typewriter` font: an elite manual (`/Pitch 12`) with a worn ribbon, so every strike is its own (the weave shows through the ink, edges feather, some counters clog, a few letters land high or light). The memo header, the tab stops, a strikeover (`<o>was\|were</o>`), an x-out, underscores and the redactions (`<r>..</r>`, drawn by `rel-redact` as marker bars) are markup in one text string. `rel-page` wraps the typing: aged and folded paper, two punch holes, the scanner's tilt and lamp, then one photocopy generation, which turns the paper white, greys the stamps and closes the type up a little. The date stamp, the released-in-part stamp and the handwritten initials and note (`lib/handscript.ps`) are marks over the page, so the copy copies them. The office, people and project are invented. |
| [The Man from 2026](https://jeromebanks.github.io/postscript_interpreter/gallery-time-traveler-memo.html) | [`time_traveler_memo.ps`](time_traveler_memo.ps) | Typed with `tw-type` on the `/Typewriter` font, an elite manual with a worn ribbon (every strike its own: the weave in the ink, feathered edges, a few light or high letters, the `e` clogged on this machine). The memo-for-the-record header, tab stops, two x-outs, a strikeover (`<o>offcie\|office</o>`) and an underscore are markup in one text string. The same machine then types the control lines in black and, with its ribbon lifted to the red half (`tw-set` with a red `/Ink` and a little `/Bleed` of black into the tops of the letters), the classification at the head and foot of the sheet. `rel-page` with `/Generation 0` makes it the ribbon original, never copied: aged legal paper, two punch holes, a gem clip, the scanner's tilt and lamp. The registry stamp, a regrading box, the routing initials and the margin note (`lib/handscript.ps`) are marks over the page. The office, people and places are invented; the only real names are in the man's own testimony. |

### Data visualization

Charts and surfaces drawn as clean vector figures.

| Piece | File | How it is made |
|---|---|---|
| [Ripple Range](https://jeromebanks.github.io/postscript_interpreter/gallery-ripple-range.html) | [`ripple_range.ps`](ripple_range.ps) | Two ripple sources, each fading with distance, are added into a height field. The surface is swept row by row under `graph.ps`'s `project3` camera, each row a filled ridge that hides the ones behind it, painted back to front (the painter's algorithm). |
| [Field Notes](https://jeromebanks.github.io/postscript_interpreter/gallery-field-notes.html) | [`field_notes.ps`](field_notes.ps) | `dataviz.ps`'s `barchart` and `linechart` share one category axis, plotting weekly sightings against a temperature trend. A species-mix donut chart has a hand-drawn legend, and all of the lettering is in the HandScript font. |

### Apparel and design

Print artwork for shirts and jerseys.

| Piece | File | How it is made |
|---|---|---|
| [Keepin' it Real](https://jeromebanks.github.io/postscript_interpreter/gallery-keepin-it-real.html) | [`keepin_it_real.ps`](keepin_it_real.ps) | The shirt front is set in both groovy faces, Bagel Fat One (the chosen one) and Chewy, with the same composition, seed and palette, and filled with fillkit's plasma through lettering's `/plasma` treatment. The words `I'm` and `Keepin' it` tuck against the `ea` of `Real` through headline's `hllayout`, and the back's typewriter punchline sits below. |
| [Varsity Backs](https://jeromebanks.github.io/postscript_interpreter/gallery-varsity-backs.html) | [`varsity_backs.ps`](varsity_backs.ps) | Each jersey carries `lib/jersey.ps`'s arched name over a large number with classic layered lettering: a white or gold fill, a thin contrasting inner outline and a dark outer one, with tracked capitals on the arch. The garment itself (mesh fabric, rib collar, yoke, side panels, sleeve and hem stripes, size tag, hanger) is drawn in the piece rather than in the library; the library only sizes the lettering for a print rectangle. |

### Drawing (dry media)

Chalk, crayon and charcoal on slate and toothed paper.

| Piece | File | How it is made |
|---|---|---|
| [Lecture 14](https://jeromebanks.github.io/postscript_interpreter/gallery-chalkboard-lecture.html) | [`chalkboard_lecture.ps`](chalkboard_lecture.ps) | Three sliding slate panels carry one derivation. The Taylor series are recalled with braces under the first terms naming the n each comes from, and the powers of i set in two columns with a rule mod 4. Then x = iθ is substituted, the = where the signs flip starred and answered in pink, and the series regrouped, a blue brace under each bracket naming it cos θ or sin θ, before the result is boxed twice in yellow. A freehand unit circle, the quarter turns tabled in wandering columns, and Euler's identity circled finish it. The formulas are `lib/mathkit.ps` layouts (`\underbrace`, `\overset`, `\pmod`, aligned columns with `/ColumnWander`, and `mkbrace`) inked with `lib/drychalk.ps`'s `dcchalk`, on `dcboard` panels that still carry the last lecture (the Gaussian integral and a bell curve) as ghost writing under eraser swipes. The hand leans further and loosens panel by panel. |
| [My Family and Our House](https://jeromebanks.github.io/postscript_interpreter/gallery-crayon-drawing.html) | [`crayon_drawing.ps`](crayon_drawing.ps) | Built the way a child builds one, from `lib/drycrayon.ps` on drykit's toothed surface. The sky is a band of blue scribble across the top and the grass a band along the bottom, both laid in side-by-side patches that run past their edges and leave white seams. A sun far too big, a red house with a brown roof, a path, an apple tree, flowers, birds, a smoking chimney and four stick figures about as tall as the house follow, and the name is set in strokeglyphs capitals with a backward S, each letter its own size and lean. Outlines go on last. The paths come from `dywobble`, `dyscribble`, `dyloop`, `dyrays`, `dygrass` and `dyfigure`, inked by `dycrayon` in the colours of a 16-crayon box. |
| [Study after Berger, 1864](https://jeromebanks.github.io/postscript_interpreter/gallery-lincoln-charcoal.html) | [`lincoln_charcoal.ps`](lincoln_charcoal.ps) | `lib/etching.ps`'s `et-tonemap` reads the photograph into a 240 by 300 grid of darkness, and drycharcoal draws to it in a student's order on drykit's drawing paper: faint `dhsketch` construction lines (head oval, centre line, brow, eye, nose and mouth lines, shoulders), `dhsmudge` stump tone inside the dark masses, `dhhatch` to the photo's values (fine and short in the face, finer still at the features, looser vine strokes in the coat), compressed-charcoal and pencil accents at the brows, eyes, nose and mouth, and hard-eraser `dhlift` streaks in the hair and on the lapel. The coat and the ground fade out raggedly, as in a real study. |
<!-- gallery:table:end -->

Nightfall, Three Cuts is a deliberate exception. The sibling printmaking
libraries (hatchkit, stipplekit, surfacekit, halftonekit) each ship with a
specimen sheet only, on the rule that a primitive gets a specimen, not a
gallery card. Nightfall was asked for explicitly, as one composition that
puts printkit's three presets to work together, so it is a one-off and not a
new default.

## Reference portrait study

`references/mochi.jpg` is the original 3000×4000 photograph used for
**Mochi in Denim Blue**. The painting preserves the reference's most
recognizable anchors — upright ears, orange-sable and cream markings,
oversized reflective eyes, compact charcoal nose, and surrounding denim
blue — while deliberately simplifying individual hairs into directional
loaded-brush masses. The photograph's close framing becomes a centered
canvas portrait; the exact clothing folds become a cross-woven painted
ground. The result is an interpretation, not a traced or photorealistic
copy: `pkoil` supplies the wet planes and bristle ridges, while `pkdry`
softens selected contour passages like a fan brush dragged through wet
paint.

`references/lincoln_berger_1864.jpg` is a 480 × 600 greyscale crop of
the head and shoulders from Anthony Berger's photograph of Abraham
Lincoln, taken at Mathew Brady's Washington gallery on 9 February 1864
(Ostendorf O-91). Source: the Library of Congress print, via Wikimedia
Commons, [File:Abraham Lincoln O-91 by Berger, 1864.png](https://commons.wikimedia.org/wiki/File:Abraham_Lincoln_O-91_by_Berger,_1864.png).
It is in the **public domain** (a US work published before 1929, and
marked public domain by Commons and the LoC). The Commons/LoC description
names it the model for the five-dollar bill's portrait. **Study after
Berger, 1864** reads its tones from this file; `docs/dev/design/drymedia/lincoln_compare.png`
sets the two side by side over real student studies from the #233
reference board.

## Viewing the gallery

```sh
./gallery/show.sh          # rendered stills, one at a time (Quick Look)
./gallery/show.sh --live   # each piece draws itself in a pscat window;
                           # close the window to advance
```

Or run any piece by hand — page sizes match each file's `%%BoundingBox`,
and `--speed` (interpreter steps per frame) sets the drawing tempo:

```sh
cargo run --release -- --page 700x700 gallery/golden_bloom.ps
cargo run --release -- --page 620x820 --speed 400 gallery/ember_tree.ps
cargo run --release -- --page 620x800 --speed 3000 gallery/fern.ps
```

The fern wants a high speed (it plots 48k dots); the tree is nice slow.
One quirk worth knowing: pieces built from a few large `stroke`s (the
rose, the waves) appear in big steps — paint happens at `stroke`/`fill`
time, exactly like real PostScript.

## Re-rendering the stills

`gallery/renders/*.png` are 2× supersampled: each was rendered with
`2 2 scale` prepended and a doubled `--page`, e.g.:

```sh
{ printf '2 2 scale\n'; cat gallery/golden_bloom.ps; } > /tmp/hi.ps
cargo run --release -- --page 1400x1400 --png gallery/renders/golden_bloom.png /tmp/hi.ps
```

Every image is deterministic: same file, same pixels. Change the `/seed`
in the tree or fern and a different individual grows.
