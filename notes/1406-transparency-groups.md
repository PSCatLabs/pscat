## Transparency groups: `begingroup` / `endgroup` (issue #1406, 2026-11-24)

`setalpha` and `setblendmode` apply to each mark on its own. A transparency
group (PDF's term, PDF 32000-1 section 11.4) draws several marks as one
thing and composites the result once, with one opacity and one blend mode:
a drawing faded as a whole (where two of its shapes overlap the overlap is
no darker), or a Multiply overprint of a whole ink layer. psanim did this
outside the interpreter by swapping the canvas, which works for PNG but can
never be written as PDF or SVG. This puts the group in the graphics model.

### Operators

```postscript
<< /Alpha 0.5 /Blend /Multiply >> begingroup
  ... any drawing ...
endgroup
```

- `dict begingroup -`, `- endgroup -`. pscat extensions in `systemdict`,
  like `setalpha` and `setblendmode` (undotted, as those are). Probe with
  `systemdict /begingroup known`.
- `/Alpha` (number, clamped to 0..1, as `setalpha` clamps) and `/Blend`
  (`/Normal` or `/Multiply`, any other name `rangecheck`, as
  `setblendmode`). Both optional. A wrong type is `typecheck`. Other keys
  are ignored, as gs ignores keys it does not use in a parameter dict
  (an image dictionary, a page device request); reserving the names PDF uses
  (`/Isolated`, `/Knockout`) for later.
- `endgroup` closes the innermost group opened at the current level: the
  page, or the pattern `PaintProc` running now. With none open there it
  does nothing, as `grestore` with nothing saved and `cliprestore` with
  nothing saved do. Not an error, because a group can end behind the
  program's back (a `showpage` or a `restore` ends it, below), and a
  program written `begingroup ... showpage endgroup` must not then fail.
  An `endgroup` in a `PaintProc` never closes a group of the page around
  it: that group's layer is not the canvas the tile is drawn on.
- Errors: a non-dictionary is `typecheck`; a bad `/Alpha` or `/Blend` as
  above; a 65th open group, or open layers past 2^28 pixels together (1 GiB),
  `limitcheck`; a layer past `--max-memory`, `limitexceeded (memory)`.

### How `/Alpha` composes with `setalpha` and the blend mode

The PDF model, which is also how gs's own `.begintransparencygroup` reads
the graphics state: painting a group XObject (`Do`) composites it with the
alpha constant and blend mode current at the `Do`, and at the start of the
group's content "the blend mode ... shall be initialized to Normal, the
soft mask to None, and the current stroking and nonstroking alpha constants
to 1.0" (PDF 32000-1, 11.6.6). So:

- The group composites with `currentalpha x /Alpha` (default `/Alpha` 1)
  and with `/Blend` if given, else `currentblendmode`.
- Inside the group, alpha starts at 1 and the blend at `/Normal`; marks in
  the group may `setalpha`/`setblendmode` freely and those composite within
  the group.
- `endgroup` brings back the graphics state `begingroup` had (alpha and
  blend included), as `grestore` does.

`0.5 setalpha << >> begingroup` therefore fades the group to 0.5, the same as
`<< /Alpha 0.5 >> begingroup`, and `0.5 setalpha << /Alpha 0.5 >>
begingroup` gives 0.25, as nested PDF groups with `ca 0.5` each would.

### Isolated, non-knockout

The group is **isolated** (its content composites onto a transparent
backdrop, then the result onto the page) and not knockout. In the raster the
layer starts transparent, which is isolation; PDF's default is
non-isolated, so the form's group dictionary says `/I true`; in SVG a `<g>`
with `opacity` below 1 or a `mix-blend-mode` is a stacking context, which is
isolation, and a group with neither gets `isolation:isolate` so the three
outputs agree when content inside uses `/Multiply`. A non-isolated group
would need a copy of the backdrop per group; nothing asked for it.

### Graphics state, nesting, clip

- `begingroup` is a `gsave`; `endgroup` restores to that level and pops it,
  whatever the content did in between (an unmatched `gsave` inside is
  closed).
- The group is a **barrier** for `grestore` and `grestoreall`, as a `save`
  is (#505): a `grestore` at the group's own level gives back the group's
  starting state without popping it, so `endgroup` still finds its level.
  Whichever of the innermost `save` and the innermost group was opened last
  is the barrier.
- Groups nest; each `endgroup` composites the innermost into the one around
  it (or the page).
- The clip in force at `begingroup` clips everything in the group (marks are
  clipped as they are drawn into the layer; the layer is composited as it
  is). `clip`, `initclip`, `clipsave`/`cliprestore` inside the group work
  as anywhere and end with it.

### Page boundaries, `restore`, end of job

A group cannot span a page (a PDF form XObject belongs to one content
stream), and an open group's marks must not be lost:

- `showpage`, `copypage` and `erasepage` first end every group open at the
  page's level, innermost first, as if `endgroup` were called; then they do
  their usual work. (`copypage` then goes on drawing outside the groups.)
- `restore` of a `save` made before a `begingroup` ends that group (and
  any inside it) as it unwinds the graphics state past it; so does the end
  of a Type 3 `BuildChar`/`BuildGlyph` or a pattern `PaintProc` that left
  one open.
- When the program ends (`step_n` reports done, an uncaught error or a
  limit stops it, or `quit`) every open group is ended, so its marks reach
  the page and every output. Consequence: in the REPL a group does not
  carry from one input line to the next.
- `setpagedevice` inside a group is `invalidaccess`, as it is in a
  pattern's `PaintProc`: the layer is the page's size.

### Raster

`begingroup` swaps the canvas for a transparent layer of the same size and
the same device coordinates, so every painting path (fills through the
16-bit shadow, strokes, images, `shfill`, patterns, glyphs) draws into it
with no change, and the clip masks (page-sized) fit it. The 16-bit shadow
(#731) is swapped too: the layer gets its own, so a stack of faint marks in
a group still reaches the arithmetic colour. `endgroup` composites the
layer onto the canvas below with the group's alpha and blend
(premultiplied source-over or Multiply, tiny-skia's formulas, in f32),
skipping the pixels the layer left transparent, and puts the canvas back.

Why the full page and not a layer cropped to the clip: clip masks, the
coverage scratch mask and the shadow are all page-sized and indexed by
device pixel; a cropped layer would need every one re-made in the layer's
coordinates. A cropped layer (and a composite over only the painted box)
is a possible follow-up if memory or time per group matters.

### Limits

- Memory: each open group holds one page of RGBA (4 bytes a pixel) plus its
  shadow rows. `begingroup` asks `--max-memory` for the layer before
  allocating it (as a pattern capture does) and fails with `limitexceeded
  (memory)`; the shadow's rows are asked for by the fills, as on the page.
  At most 64 groups may be open at once (the pattern capture depth), and
  `begingroup` past that is `limitcheck`.
- Steps and time: making the layer and compositing it are work proportional
  to its pixels, charged to `--max-steps` a step per 1024 pixels, as `fill`
  charges its rasteriser.

### `--bands`

A band run draws the whole page and keeps its rows; the layer is the whole
page too. The composite is limited to the band's rows (others are thrown
away), as the shadow already is.

### PDF

`begingroup` sets the page's content stream and resource lists aside (the
pattern capture's mechanism, without its matrix or colour rules) and
records into an empty stream. `endgroup` stores what was recorded as a form
XObject, `/Subtype /Form /BBox [0 0 W H] /Group << /S /Transparency /I true
/CS /DeviceRGB >>`, in the content's own (device) coordinates, and writes
`q /GSn gs /Xk Do Q` into the stream below, where `/GSn` is the group's
`ca`/`CA`/`BM` (omitted when 1 and Normal). An empty group writes nothing.

### SVG

`begingroup` marks a place in the body; `endgroup` wraps what was recorded
since in `<g opacity="a" style="mix-blend-mode:multiply">...</g>` (each part
only when not the default; `isolation:isolate` when neither is set). An
empty group writes nothing.

### Ghostscript

gs 10.07.1 has `.begintransparencygroup` in `systemdict` but no
`.endtransparencygroup`, and calling it from PostScript is `unregistered`
(it needs the pdf14 compositor its PDF interpreter pushes):

```
$ gs -q -dNOSAFER -sDEVICE=png16m -o x.png -c "<< >> 0 0 100 100 .begintransparencygroup"
Error: /unregistered in --.begintransparencygroup--
```

So there is no gs spelling a PostScript program can use, and nothing to be
compatible with; a dot-named alias would only make a program fail later
in gs. gs is still the oracle for the result: it renders pscat's PDF
(transparency group XObjects are core PDF 1.4), and the tests compare that
with pscat's own raster.

### Not done

Blur and soft masks (`/SMask` on a group) are out of scope (#1406). No
`/Isolated false` or `/Knockout true`.
