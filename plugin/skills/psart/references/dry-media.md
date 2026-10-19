# Dry media: chalk, crayon, charcoal (`lib/drykit.ps` and friends)

Paths such as `examples/...`, `gallery/...` and `docs/...` name files in the
[pscat repository](https://github.com/jeromebanks/postscript_interpreter); an
installed pscat does not include them. Every `(lib/...) run` works anywhere:
the library is compiled into `pscat`.

## Chalk (`lib/drychalk.ps`, after artkit, paintkit and drykit)

For a chalkboard, use `lib/drychalk.ps`, not a pen stroke in a chalk
colour (that is `gallery/hundred_lines.ps`'s flat look). Load
`lib/handscript.ps` and `lib/strokeglyphs.ps` too for handwriting: `sgpath`
gives skeleton paths, and `dcchalk` inks them. A character with no
skeleton (a superscript two, say) raises `sg-missing-glyph:<char>` rather
than vanishing; write it another way, or pass `/Strict false` to let it
advance blank.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/drychalk.ps) run
40 40 520 300 << /Surface /greenboard /Tray 12
    /Ghost { (last lecture) 60 250 << /Size 30 >> sgpath } >> dcboard
/white dcchalkcolor
newpath (fresh notes) 60 180 << /Size 32 >> sgpath << /Width 1.8 >> dcchalk
newpath 58 170 moveto 260 172 lineto << /Tilt 85 /Side 5 >> dcchalk
```

- **`dcboard`** (`x y w h opts`) paints the board and makes it the
  surface. It draws:
  - a dust haze that is never flat;
  - `/Ghost`, a path procedure for the last lecture's writing, drawn
    erased;
  - eraser `/Swipes`, darker `/Wipes`, lighter `/Smudges`;
  - `/Ledge` dust and an optional `/Tray`.

  Call it once per board, before the chalk.
- **`dcchalk`** treats each subpath as one stroke. It lands heavy, tires in
  the middle, lifts thin, sheds dust and differs a little from its
  neighbours. `/Tilt` 80-90 with `/Side` gives broad side-of-chalk
  underlines and shading.
- **`/name dcchalkcolor`** picks the chalk: `/white /yellow /pink /blue
  /green /orange`.
- Real chalk is grey, not white: the defaults (`/Strength 0.55`) were
  measured against photographs. Raising it gives the fake-font look.
- Cost: a 42pt line is about 0.7s at 2x, so keep a board to about a dozen
  lines. `examples/drychalk_specimen.ps` shows everything.

## Crayon (`lib/drycrayon.ps`)

Chalk, crayon and charcoal sit on `lib/drykit.ps` (design:
`docs/dev/design/DRYMEDIA.md`): a stick catches only on the surface's tooth, so
light pressure breaks into streaks along the stroke. Crayon is a medium
file over it. Load artkit, paintkit, drykit, then drycrayon:

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run (lib/drycrayon.ps) run
<< /Seed 4 >> dypaper                       % crayon paper; paints nothing
0.97 0.96 0.93 setrgbcolor clippath fill    % so paint the paper yourself
/blue dycolor                               % the 16-crayon box (dybox, dycolors)
newpath 40 40 moveto 200 80 lineto << /Touch 0.7 >> dycrayon
```

- **`dycrayon`** inks the current path in the current colour: a dense core
  in a ragged rim, a child's pressure (a press on landing, a fade, an
  uneven wander), and wax build-up where the stroke turns. `/Touch` is
  drykit's pressure: 0.6 leaves about a third of the paper showing in a
  scribble fill, 0.9 is nearly solid. A later pass over the same area is
  `/Pass 2`, `3`, ...; a colour over another takes `/Over [r g b of the one
  beneath]`, which lets it show through. `/Worn` 0 is a new point, 1 a
  worn flat (wider). drykit can't read the page back, so passes and
  layering are declared, not detected.
- **Child motions** build paths and paint nothing, with paintkit's motion
  contract (each replaces the current path and draws only from `frnd`):
  `dywobble` (shaky, overshooting lines), `region opts dyscribble`
  (back-and-forth fill past the edges, with gaps), `cx cy r opts dyloop`
  (a circle that doesn't close), `x y h opts dyfigure` (a stick figure, or
  `/Tadpole true`, with `/Hair`), `cx cy r opts dyrays` (sun rays),
  `dygrass` (blades along a line, `/Zigzag true` for one continuous line).
  Each feeds `dycrayon`: `40 300 200 60 screct << >> dyscribble << >> dycrayon`.
- Cost: every mark is a clip of tooth knuckles, so a big scribble fill
  costs about a second at 2x. Publish PNG, not SVG (SVG carries every
  knuckle). `examples/drycrayon_specimen.ps` and
  `examples/drycrayon_motion_demo.ps` show everything.

## Charcoal (`lib/drycharcoal.ps`, on drykit)

```
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/hatchkit.ps) run (lib/drycharcoal.ps) run
<< /Surface /drawing >> dksurface                 % charcoal breaks up in specks (#313)
0.11 0.10 0.10 setrgbcolor                        % the pigment is the current colour
```

- `opts dhstroke` — the current path in `/Medium /vine` (grey, dusty),
  `/compressed` (dark, dense) or `/pencil` (sharp; hard grades break into
  dots). `/Grade` 0 (hard) to 1 (soft). Any other `dkstroke` key overrides
  the preset.
- `opts dhhatch` — hatches the current path's region to `/Value` (0..1, or
  `{x y -> value}`): up to `/Layers` crossing layers of hand strokes, all in
  one `dkbatch` so crossings stay dark. It clips to the region. Overlapping
  regions go in **one** `{ ... } dkbatch` (#312): each call keeps its own
  region, and the crossings stay dark. As two separate calls they are two
  batches, and at `/Levels 2` or more the second one's faint level greys
  the first's crossings.
  Your own `gsave region clip ... grestore` around `dkstroke` calls inside
  a batch is kept as well (#444), nested and `eoclip` clips included.
- `opts dhsmudge` — the path is a stump's track: a wide, soft stroke that
  fills the tooth, leaving `/Value` between paper and the current colour.
  drykit cannot read the page, so give `/Under` (the tone beneath) when it
  is not paper. Over hatching, a moderate `/Touch` lets the hatch show.
- `opts dhlift` — an eraser's track: `/Eraser /kneaded` (grey, soft, never
  back to paper) or `/hard` (a narrow, crisp line). `/Under` defaults to the
  current colour.
- Smudges and lifts may sit inside a `dkbatch` (#393): they are drawn after
  the batch's strokes, in call order, over all of them, even hatching made
  later in the batch. To hatch over a lift, hatch in a later batch.
- `opts dhsketch` — faint construction lines, laid `/Passes` times, each
  shifted by up to `/Shift`; draw the drawing over them.
- None of them draws from your `rand` stream. **Cost:** hatching costs about
  0.07 ms per square point per layer in release pscat at 2x (a 120 x 120 pt
  square is 1.3 s at one layer, 4 s at four), so keep hatched areas modest.
  In Ghostscript, any `scatter` (vine charcoal's fallout) makes the drykit
  work after it about 2x slower (#311), so draw vine strokes late.
  `examples/drycharcoal_specimen.ps` shows all of it.
