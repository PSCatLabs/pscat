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
- The clip in force at `begingroup` clips the group's marks as they are
  drawn into the layer (the layer is composited as it is), unless an
  `initclip` inside the group lifts it, as it would anywhere; raster, PDF
  and SVG agree on that. `clip`, `initclip`, `clipsave`/`cliprestore` inside the group work
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
- When the program ends, every open group is ended, so its marks reach the
  page and every output and the canvas is the page again. A program that
  is done (or `quit`s) ends them in `step_n` *before* the slice's limits
  are looked at, so their work is held to them; an aborted program (an
  uncaught error, or a limit found at the end of a slice or at the start
  of the next one) ends them in `unwind_all`, after the pattern captures
  are unwound, on every path (Codex round 1, finding 2). A group closed
  after a time limit has fired stops compositing at once: the layer is
  dropped and the page keeps what it had, so the abort stays cheap.
  Consequence: in the REPL a group does not carry from one input line to
  the next.
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
skipping the rows and pixels the layer left transparent, and puts the
canvas back.

The composite goes through the canvas's 16-bit shadow, as a translucent fill
does (Codex round 1, finding 4): the source is the layer's own shadow where
it still agrees with the layer's 8-bit pixel, the destination the canvas's
shadow, and the result is kept at 16 bits and rounded to the page. So 1000
groups of `/Alpha 0.001` white over black reach level 161 (1 - 0.999^1000 =
0.632), the arithmetic and the PDF's colour, where an 8-bit composite adds a
quarter of a level each time and stays black; and faint marks inside a
group keep their precision into the page. The composite falls back to 8
bits where a fill would (inside a pattern capture, or when the shadow is
off under `--max-memory`, or its new rows would not fit the cap).

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
  to its pixels (the composite counts every row of the layer it scans,
  plus the pixels of the rows it blends), charged to `--max-steps` a step
  per 1024 pixels, as `fill` charges its rasteriser. On every path (Codex
  round 1, finding 3): `endgroup` charges its own; a group ended by a page
  boundary, a `restore`, the end of a glyph or `PaintProc`, or the end of
  the program puts its work in `Gfx::take_group_work`, which the
  interpreter counts with the decode filters' and the mesh shadings' work
  when it next looks at its limits. The composite looks at `--max-time`
  (and the steps, for `endgroup`) every 64 rows and stops at a limit; the
  group is closed either way, its rows so far composited.
- PDF memory: closing a group stores its content stream as a form, one
  copy of it (the form's `cm` is written when the group opens, so nothing
  is reformatted), asked of `--max-memory` first (Codex round 1, finding
  1). A refusal drops the group from the PDF, leaves the stream below
  intact, and stops the run with `limitexceeded (memory)`.

### `--bands`

A band run draws the whole page and keeps its rows; the layer is the whole
page too. The composite is limited to the band's rows (others are thrown
away), as the shadow already is.

### PDF

`begingroup` sets the page's content stream and resource lists aside (the
pattern capture's mechanism, without its matrix or colour rules) and
records into an empty stream. `endgroup` stores what was recorded as a form
XObject, `/Subtype /Form /Group << /S /Transparency /I true /CS /DeviceRGB
>>`, and writes `q <D⁻¹> cm /GSn gs /Xk Do Q` into the stream below, where
`/GSn` is the group's `ca`/`CA`/`BM` (omitted when 1 and Normal). The form's
space is the stream's default space (the page's y-up points, or a pattern
capture's pattern space): its content starts with `<D> cm`, D mapping the
canvas's device coordinates there, and its `/BBox` is the canvas in that
space. A pattern's `/Matrix` maps to the default space of the stream that
paints with it, which inside a form is the form's (PDF 32000-1, 8.7.2), so a
pattern painted in a group is placed as on the page; with the form in device
coordinates gs left such a pattern out (found by the gs round trip). An
empty group writes nothing.

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
