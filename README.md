# pscat

pscat is a PostScript interpreter written in Rust. PostScript is a programming
language for drawing pages; pscat runs your program and shows the picture as it
is drawn, in a live window, or renders it to PNG, SVG or PDF. It also runs in a
browser tab, and it ships with PostScript libraries for making art:
generative patterns, paint and watercolor, printmaking, lettering, charts and
more. It was built for fun, not as a Ghostscript replacement, though it aims
for broad PostScript coverage over time.

**[jeromebanks.github.io/postscript_interpreter](https://jeromebanks.github.io/postscript_interpreter/)** has the docs, the full gallery, a specimen of every font,
and a live playground that runs the interpreter in your browser, no install.

<!-- gallery:highlights:start -->
<p>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-cathedral-rose.html"><img src="gallery/renders/cathedral_rose.png" width="32%" alt="Cathedral Rose, a lace-like rosette woven from straight red and gold chords inside a gold ring on a dark purple ground"/></a>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-ring-of-type.html"><img src="gallery/renders/ring_of_type.png" width="32%" alt="Ring of Type, a sentence repeated around eleven concentric rings that shrink toward a large ampersand at the centre"/></a>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-hortus.html"><img src="gallery/renders/hortus.png" width="32%" alt="Hortus Machinalis, a herbarium plate with three branching plants labelled in Latin-style captions on cream paper"/></a>
</p>
<p>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-ripple-range.html"><img src="gallery/renders/ripple_range.png" width="32%" alt="Ripple Range, a three-dimensional surface drawn as stacked ridges that shade from orange at the back to blue at the front, with a pale dip where the ripples meet"/></a>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-nightfall-triptych.html"><img src="gallery/renders/nightfall_triptych.png" width="32%" alt="Nightfall, Three Cuts, a moonlit ridge over water in three print styles: engraved sky and water, woodcut mountains and linocut reeds"/></a>
  <a href="https://jeromebanks.github.io/postscript_interpreter/gallery-alpine-lake.html"><img src="gallery/renders/alpine_lake.png" width="32%" alt="Alpine Lake, snow-capped peaks reflected in a still lake under a criss-cross brushed sky, with evergreens in the foreground"/></a>
</p>
<!-- gallery:highlights:end -->

## Quick start

You need a stable [Rust toolchain](https://rustup.rs). Or skip the build and
download a bundle from the
[releases page](https://github.com/jeromebanks/postscript_interpreter/releases).

```sh
cargo run -- --page 500x500 examples/sierpinski.ps   # watch it draw, live
cargo run -- --png out.png examples/sierpinski.ps    # render to a PNG instead
cargo run -- -i                                      # REPL + window: type PostScript, watch it draw
```

A taste of the REPL:

```
PS> 40 2 add =
42
PS> mark 1 2 3
PS<4> pstack
3
2
1
-mark-
```

To try it with nothing installed, open the
[playground](https://jeromebanks.github.io/postscript_interpreter/playground.html). New to PostScript? Start with
[Learn PostScript](https://jeromebanks.github.io/postscript_interpreter/learn.html). More sample programs live in `examples/`, and finished pieces in
[`gallery/`](gallery/README.md).

## Guides

Everything else is in [`docs/guides/`](docs/guides/README.md):

- [Getting started](docs/guides/getting-started.md): build, install, command-line flags, output formats
- [pscat in the browser](docs/guides/browser.md) and [for AI agents](docs/guides/agents.md)
- [Fonts](docs/guides/fonts.md) and [handwriting](docs/guides/handwriting.md)
- [artkit](docs/guides/artkit.md): the generative-art toolkit, and [style packs and page templates](docs/guides/style-packs-and-templates.md)
- [Painting](docs/guides/painting.md) and the [landscape cookbook](docs/guides/landscape-cookbook.md)
- [Printmaking and etching](docs/guides/printmaking.md) and [dry media](docs/guides/dry-media.md)
- [Lettering, headlines and apparel](docs/guides/lettering.md)
- [Charts and plots](docs/guides/charts.md)

## For AI agents

pscat works well from a coding agent. The skills in `.claude/skills/` (`pscat`,
`psart`, `psviz`) are plain Markdown any agent can read, and `pscat-mcp` is an
MCP server with four tools. See the [agents guide](docs/guides/agents.md).

## Contributing

Start with [`AGENTS.md`](AGENTS.md) for conventions and [`SDLC.md`](SDLC.md)
for how changes land. [`docs/dev/HANDOFF.md`](docs/dev/HANDOFF.md) orients you in the code,
[`docs/dev/ARCHITECTURE.md`](docs/dev/ARCHITECTURE.md) explains the design, and
[`docs/dev/`](docs/dev/README.md) has the contributor notes, such as how to
preview the website. [`docs/archive/`](docs/archive/README.md) holds the project's history, including
the original stage-by-stage [status log](docs/archive/STATUS_LOG.md).

## Why

I used to write raw PostScript by hand in college and send it straight to
a LaserWriter. This is a from-scratch Rust interpreter built to relive
that — watching a hand-written recursive PostScript program draw itself,
live, without depending on a decades-old C codebase to do it.

<!-- sdlcify:managed:start -->
## SDLC

See [`SDLC.md`](SDLC.md) for how this repo's development lifecycle
works (issue → branch → PR → review → merge → cleanup) and the
branch-protection/merge policy currently enforced on GitHub — generated
and kept in sync by the `sdlcify` skill.
<!-- sdlcify:managed:end -->
