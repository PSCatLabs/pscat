# When to use pscat, an image model, or an LLM on its own

You can get a picture three ways today:

- **an image model**: FLUX from Black Forest Labs, Google's Nano Banana
  (Gemini image), OpenAI's gpt-image, Imagen, Midjourney, Stable Diffusion;
- **a frontier LLM drawing on its own**: its built-in image output, or code it
  writes in SVG, HTML/CSS or matplotlib;
- **pscat**: a PostScript program, written by you or by an agent, rendered to
  PNG, SVG or PDF.

They are good at different things. This guide says which to pick, and why,
and is honest about where pscat is the wrong tool.

What it says about other tools is general and current as of November 2026.
These tools change quickly; check the one you are using. We have run no
benchmarks between them, so this guide quotes none.

## The short answer

**Use an image model** when the picture is the whole point and no part of it
must be exactly right: a photo-like scene, a face, light and atmosphere, a
painterly mood image, concept art.

**Use pscat** when something in the output must be exact and checkable:
words spelled right, a QR code that scans, numbers that match the data, a
layout that prints at size. Also use it when you need the result again
tomorrow, unchanged, or with one edit.

**Use pscat from a text-only model** when the assistant you are working with
cannot make images itself and you need a real picture: it writes the
program, pscat renders it, and the model looks at the result and refines it.
See [Pictures from a model that can't draw](#pictures-from-a-model-that-cant-draw).

**Use an LLM's own SVG, HTML or matplotlib** for a quick one-off sketch or
diagram in a tool you already have open.

**Use more than one** when you want both looks and exactness: an image model
for the atmosphere, pscat for the exact layer on top. See
[Combining them](#combining-them).

## Why not just ask an image model?

That is the fair question, so here is the case for the image model first.

### What image models do better

- **Photorealism.** pscat draws vector paths. It has no model of light,
  faces or materials, and it cannot make a photograph.
- **Rich painterly texture and mood, fast.** One sentence gets you a
  convincing oil sketch or a misty watercolor. pscat's paint and dry media
  libraries are good, but they are procedural: someone has to write the
  program, and the look is pscat's, not a model trained on millions of
  paintings.
- **Breadth.** Any subject, any style, from a prompt. pscat draws what its
  program says and nothing else.
- **Editing pictures.** Inpainting, "make the sky darker", restyling a photo.
  pscat edits its own source, not arbitrary images.
- **Speed to a first impression.** Seconds for a mood board. A detailed pscat
  painting is a program somebody has to write and refine.

If those are what you need, use an image model.

### What an image model cannot promise

Image models make pixels that look right. They have become much better at
short text, but they still give no guarantee, and nothing checks the result
for you:

- **Text.** Each character is drawn, not typeset. You find a misspelling by
  reading every word, and a re-roll to fix it changes the rest of the image
  too.
- **QR codes and other machine-read marks.** A QR code is data. A picture of
  one decodes only if every module is right.
- **Numbers.** A chart drawn by a model looks like a chart; its bars are not
  computed from your data.
- **Geometry and layout.** Margins, alignment, a 3.5 × 2 inch card with a
  safe area: a model approximates them.
- **Resolution and print.** The output is a raster at the model's size.
  Upscaling adds pixels, not detail, and there is no vector PDF to send to a
  printer.
- **Reproducibility.** A hosted model may change under you. Open-weights
  models such as FLUX and Stable Diffusion take a seed, but the same image
  also needs the same weights, sampler, software and often hardware. The
  output is the image itself, not something you can diff or hand-edit.
- **Cost and setup.** A hosted model is usually paid per image or by
  subscription, and needs a network connection. A local open-weights model is free per image but wants
  a capable GPU and a large download.

## Where pscat is the better choice

Every point below is a feature that ships in pscat today; the links go to
the guide for it.

- **Text that must be spelled right.** Text is set with real fonts: the
  standard 35 compiled in, and a catalog of 67 outline faces, including
  Korean, Japanese and Thai, as font packs ([Fonts](fonts.md)). What you
  write is what prints.
- **QR codes that must scan.** `lib/qr.ps` is a native QR encoder (all 40
  versions, levels L, M, Q and H) that draws the symbol as vector geometry
  ([QR codes](qr-codes.md)). Its PNG, SVG and PDF exports are checked in the
  test suite against two independent decoders. (QR only for now: pscat has
  no 1D barcodes.)
- **Math and charts that match the numbers.** `pscat chalk` sets a LaTeX
  subset on a chalkboard, and `lib/mathkit.ps` lays out the same math for
  any pen ([Dry media](dry-media.md#from-latex-to-a-board-pscat-chalk)).
  `lib/dataviz.ps` and `lib/graph.ps` draw bar, line, area, scatter and pie
  charts and function plots from your data ([Charts](charts.md)).
- **Print-ready vector output at any size.** `--svg` and `--pdf` export the
  same drawing as vectors; `--dpi 300` renders a PNG that records its dpi,
  so it prints at true size. A PDF takes its title and author from the
  program's `%%Title:` and `%%For:` comments
  ([Getting started](getting-started.md)).
- **Fixed layout.** Page templates for cards, letters, certificates,
  invitations and posters ([Style packs and templates](style-packs-and-templates.md)),
  a typewriter for letters and memos, and a two-sided business card with a
  QR code in the gallery (`gallery/pscat_card.ps`).
- **Reproducible, diffable, editable.** The artifact is a plain-text
  program. Under `N srand` (or a library's `/Seed`) the same program and
  seed give the same picture; for artkit the same pixels on macOS, Linux and
  in the browser build ([artkit](artkit.md)). Change one word or one colour
  and re-render, and only that changes. `--sweep-seed 1:12` renders a dozen
  variations in one run, and `pscat sheet` puts several programs side by
  side for review ([Getting started](getting-started.md)).
- **Checkable by a machine.** `--lint-strict` flags a blank page or
  operands left on the stack, and a PostScript error comes back with the
  standard error name. An agent can render, look at the PNG, and fix the
  program ([pscat for AI agents](agents.md)).
- **Local and free per image.** One binary that runs on the CPU, with no
  GPU and no account. Once installed it renders offline (the font packs and
  the agent plugin's first run download once). The code is MIT or
  Apache-2.0.

### Painterly media, the pscat way

pscat paints too: watercolor washes on paper, oil and palette knife,
dry brush, chalk, crayon and charcoal ([Painting](painting.md),
[Dry media](dry-media.md)). The gallery's
[Evening Stream](../../gallery/evening_stream.ps) and
[Alpine Lake](../../gallery/alpine_lake.ps) are painted with these libraries. It will not out-paint an image model on mood.
What it gives you is different:

- **Deterministic and seedable.** The watercolor wash and the paper under it
  are deterministic under `/Seed` or the caller's `srand`. The same program
  paints the same picture every time.
- **Editable source.** Move the horizon, change the pigment, make the bloom
  wetter: edit a line and re-render. You don't re-roll and hope.
- **Vector.** The wash is vector geometry, so a painted piece exports to SVG
  and PDF like everything else.
- **Composable with exact parts in one document.** A watercolor background,
  type set in a real font, a QR code that scans and a chart from real data
  can be one program and one PDF: the libraries load side by side in the
  same program.

## Pictures from a model that can't draw

Some assistants have no image output of their own. As of November 2026,
Claude is one: it reads images but does not generate them. Other assistants
do generate images; check the one you use.

pscat gives such a model a way to make real pictures. With the pscat plugin
or the `pscat-mcp` server installed ([pscat for AI agents](agents.md)), the
model writes PostScript, pscat renders it to PNG, SVG or PDF, and the
rendered PNG comes back to the model, so it can look at what it drew and
fix it: render, look, refine. There is no separate image-model account, no
API key and no per-image cost; pscat runs on your machine.

That model could also write SVG, HTML or matplotlib, and for a quick diagram
that is often enough (see the next section). pscat's edge is everything
around the drawing:

- the rendered picture comes back to the model, with any error, in the same
  loop;
- painterly media and the generative-art toolkit (watercolor, oil, chalk,
  charcoal, turtle graphics, L-systems, palettes);
- real fonts, a handwriting font and a typewriter;
- QR codes that scan, charts from data, LaTeX math;
- print-ready, multi-page PDF.

If you also have an image model, you can use both: see
[Combining them](#combining-them).

## An LLM drawing on its own

A capable LLM can also make the picture itself, two ways.

**Its built-in image output** is an image model, with the same strengths and
limits as above.

**Code it writes: SVG, HTML/CSS or matplotlib.** This is close to pscat in
spirit, and often the right call:

- SVG is vector, and it is source you can diff and edit. For an icon, a
  simple diagram or a figure a browser will show, an LLM's hand-written SVG
  is quick and needs no install.
- matplotlib is the natural choice if your data is already in Python. It is
  a mature plotting library with far more chart types than pscat's.
- HTML/CSS is the right tool for anything that ends up as a web page.

Where pscat differs:

- **Libraries for the hard parts.** A QR encoder, LaTeX math, page templates,
  a handwriting font, paint and dry media, engraving and halftone. In plain
  SVG the LLM writes those by hand, and a QR code written by hand rarely
  scans.
- **Fonts and print.** SVG text depends on the fonts of whatever displays
  it. pscat draws text from the font it was given, and writes multi-page
  PDF directly.
- **Repeatability.** The same pscat program and seed draw the same picture
  from the command line, the MCP server or the browser build.
- **A look-and-fix loop.** `pscat-mcp` returns the rendered PNG to the agent
  with any error, so it can see what it drew and fix it.

## PostScript is a program; SVG and HTML are descriptions

SVG and HTML are declarative markup: they list what is on the page. PostScript
is a full programming language: loops, procedures, recursion, dictionaries,
seeded random numbers, and path operations you can compute. That changes what
one short file can do.

- **Thousands of marks from a few lines.** A loop, a recursion or an L-system
  rule draws as many marks as you ask for. artkit's L-systems, scatter,
  tilings, fractals and path brushes ([artkit](artkit.md)) are a few lines
  each to call. In SVG every element is spelled out, or generated by a script
  in another language.
- **Libraries are procedures.** artkit, the paint and dry media, `dataviz`
  and `graph`, the page templates, the QR encoder and the chalk math are
  PostScript procedures your program calls, not files you copy from.
- **One program, three outputs.** The same program renders to PNG, vector PDF
  and SVG (`--png`, `--pdf`, `--svg`). pscat writes SVG, so picking it does
  not rule SVG out: it is another route to SVG, not a rival. (A dense
  painting or chalkboard makes a large SVG, every mark its own element;
  prefer PNG or PDF for those.)
- **Documents and print are native.** `showpage` ends a page, and `--pdf`
  writes the whole multi-page document with its title and author. HTML gets
  to paper through a browser's print path.

Where SVG and HTML are the better choice:

- interactive or animated web content, and anything scripted in the page;
- layout that reflows with the window, using CSS;
- accessibility: real text that screen readers, search and copy-paste see;
- editing in the browser's DOM or in a vector editor;
- nothing to install: every browser renders them.

## Refining by feedback loops

pscat suits a goal-driven loop on looks: render, have a critic (you, or an
art-critic agent) judge the PNG, turn that into specific feedback, edit the
source, render again, and repeat until it looks right. The psart skill is
built around exactly this render-look-refine loop, and over MCP
`render_postscript` hands the PNG straight back to the agent
([pscat for AI agents](agents.md)). This repository uses an agent critic the
same way: a render change is judged by an independent reviewer agent against
written criteria before it merges.

Why it converges:

- **Each round is a targeted change.** Move a coordinate, change one palette
  entry, the stroke count, an opacity or the seed, and everything else stays
  exactly as it was. "Move the moon 5% left" is an edit to one number.
- **Every step is a diff.** You can review a round, keep it or revert it.
- **The end result is reproducible.** The final program and seed render the
  same picture again.
- **Candidates side by side.** `--sweep-seed 1:12` or `--sweep NAME=10,20,40`
  renders one program across seeds or values in one run, and `pscat sheet`
  (the `review_sheet` MCP tool) lays several programs out on one labelled PNG
  ([Getting started](getting-started.md)).

Image models refine too: conversational edits, inpainting and image-to-image
are standard, and they work well. The general difference, as of November
2026, is that an edit re-samples the image. It can disturb parts you were
happy with (text, a face, the composition can drift), there is no direct
numeric control ("warm this wash slightly"), and the same edit does not
give the same result twice.

The two combine: in a hybrid piece (see [Combining them](#combining-them)),
generate the image-model layer once and freeze it, and do the iterating on
the pscat layer above it.

## A deterministic workflow

A pscat render is a function of the program and its seed: with the same pscat
version and the same installed font packs, the same input gives the same
pixels on every run. (A face from a font pack that is not installed renders
with a stand-in font, so install the same packs everywhere.) pscat also does its own
math rather than each platform's, so a seeded render matches on macOS, Linux
and in the browser build (rarely, a thin curve can differ by a pixel or two;
see [artkit](artkit.md)). Asking an image model twice for the same prompt
gives two different images, and a hosted model can change between versions.

That makes a pscat picture behave like code:

- **Reviewable.** A change is a text diff of the program.
- **Testable.** A render can be checked in CI. pscat's own test suite pins
  rendered output and asserts that seeded libraries are deterministic.
- **Cacheable and reproducible.** The source and seed identify the image.
  Keep them, and the same pscat version, with the same font packs, renders it
  again later, unchanged.
- **Cheap to repeat.** An LLM can write the program once; rendering it again,
  or a hundred variations of it, needs no model call at all.

It also runs in the browser. The WebAssembly build (`npm install pscat`, or
[pscat in the browser](browser.md)) renders client-side, with no server, GPU
or API call; the site's
[playground](https://pscatlabs.github.io/pscat/playground.html) is that build.

## The decision table

| Task | Use | Why |
|---|---|---|
| Photo-like scene, product shot, portrait | Image model | pscat cannot do photorealism or faces |
| Mood image, concept art, loose painterly texture | Image model | Fastest route to atmosphere; nothing in it has to be exact |
| Retouch or restyle an existing photo | Image model | pscat edits its own programs, not photos (it can turn a JPEG into an engraving) |
| Poster, flyer or card with real text | pscat (with an image-model background if you like) | Text set from fonts, layout to the point, print-ready PDF |
| QR code that must scan | pscat | Native encoder, exports checked against two decoders |
| Chart or plot for a paper | pscat or matplotlib | Values drawn from the data; matplotlib if the data is already in Python |
| Math formula, chalkboard | pscat | LaTeX subset set and drawn, same seed same board |
| Letter, certificate, invitation, memo | pscat | Page templates and the typewriter |
| Generative art you will iterate on or print | pscat | Seeded, diffable source; vector output |
| Watercolor or oil you need to edit, repeat or print large | pscat | Deterministic, editable, vector, composable with text and QR |
| You're in Claude or another text-only agent and need an actual image | pscat (plugin or MCP server) | The model writes the program, sees the render and refines it; no image-model account or per-image cost |
| Quick diagram or icon for a web page | LLM-written SVG or HTML | Nothing to install; the browser renders it |
| Many variations of one design | pscat | `--sweep-seed` and `pscat sheet` |
| You will iterate on looks with an agent critic until it is right | pscat | Each round is a small, revertible source edit with everything else fixed; candidates compare on one sheet |
| Thousands of rule-generated marks (patterns, L-systems, tilings) | pscat | Loops, recursion and seeded random numbers are in the language; SVG lists every element |
| Interactive, animated or reflowing web content | SVG, HTML and CSS | The DOM, CSS layout, scripting and accessibility |
| An image that must be regenerated identically, in CI or later | pscat (pin the version and font packs) | Same program, seed, version and fonts, same pixels; no model call |
| Rendering inside a web page, no server or GPU | pscat's WebAssembly build, or SVG | Both render client-side |
| Offline, no GPU, no per-image cost | pscat | One CPU binary, MIT or Apache-2.0 |

## pscat's weak spots

- Not photorealistic. No faces, no physically based light.
- Its painterly look is procedural. It is good, and it is recognisably
  pscat's; it will not imitate any style you can name.
- Someone has to write the program. An agent can, but a detailed painting
  takes several render-look-refine rounds, where an image model answers in
  one prompt.
- No 1D barcodes yet, and text only in the scripts its fonts cover
  ([Fonts](fonts.md)).

## Combining them

Often the best result uses both: the image model paints the atmosphere, and
pscat sets the exact layer on top (type, a QR code, a formula, a chart, the
frame) and writes the print-ready PDF. pscat can also read a JPEG photo and
turn it into an engraving ([Printmaking](printmaking.md)).

A worked hybrid example, with a side-by-side comparison of the image model
alone, pscat alone and the two together, is planned in
[issue #995](https://github.com/PSCatLabs/pscat/issues/995). Its guide will
be `docs/guides/pscat-with-image-models.md`.
