# Style packs and type

Paths such as `examples/...`, `gallery/...` and `docs/...` name files in the
[pscat repository](https://github.com/PSCatLabs/pscat); an
installed pscat does not include them. Every `(lib/...) run` works anywhere:
the library is compiled into `pscat`.

## Style packs (`lib/styles/`, load after artkit)

Four motif libraries, one per aesthetic — each registers three
palettes into artkit's `Palettes` and adds path builders (you paint
them) and painted stamps (self-contained). Every file's header lists
its full API; `examples/style_<name>.ps` is a worked specimen poster
for each.

- **steampunk** — `gear` (path, bored hub), `rivet`, `pipe`, `gauge`,
  `plateframe`; palettes /brass /verdigris /boiler; `/spmetal` dial
  picks the stamps' metal. Pair with Rye + SpecialElite.
- **psychedelic** — `rays`, `blob` (wobble circle — nest and cycle
  colors), `spiral`, `wavy`, `kaleido` (n-fold repeat about a
  center), `t rainbow`; /acid /blacklight /sherbet. Monoton on an
  `arcn` path via `pathtext` is the move.
- **scifi** — `glowstroke` (the neon seller: halo/mid/core),
  `starfield`, `planet` + `planetring` (`/sfworld` dial),
  `hudcorners`, `reticle`, `hexfield`, `gridfloor` (synthwave
  perspective floor — glowstroke it); /void /hologram /synthwave.
  Orbitron, Audiowide, VT323, PressStart2P.
- **toon** — the cel-cartoon look: `celfill` (flat fill + fat ink
  outline, the foundation), `burst`, `bubble` (speech, tail toward a
  point), `speedlines`, `dotfill` (halftone in the current path, path
  survives), `eye`, `dripbox` (slime title slab); /saturday
  /latenight /pastelpop; `/tnink` dial sets the line color. Bangers,
  ComicNeue, PermanentMarker.

Scratch prefixes sp-/py-/sf-/tn- join artkit's reserved list — don't
reuse them in your own procs.

## Type in depth

- `pscat --fonts` lists everything. The standard 35 all resolve
  (Palatino, Bookman, ZapfChancery, Symbol, ZapfDingbats…) plus ~35
  display families: `/GreatVibes-Regular`, `/Bangers`,
  `/UnifrakturMaguntia`, `/PressStart2P-Regular`, `/Creepster-Regular`…
- The Type 3 program-faces (`lib/fonts/*.ps`, load with `(file) run`)
  — /Neon, /Marquee, /Constellation, /Lapidary, /Circuitry,
  /Stitchwork, /Confetti — take dials through their scratch dicts
  (each file's header documents them).
- /Typewriter (`(lib/typewriter_outlines.ps) run (lib/typewriter.ps) run`)
  types like a real machine, for memos, letters and "found document" pieces.
  `<< /Text (...) /Width .. /Height .. /Margin .. /Spacing 1.5 /Machine 7 /Seed 3
  /PageStart { paint the paper } >> tw-type showpage` wraps, tabs, underlines
  (`<u>..</u>`), strikes over (`<x>..</x>`, `<o>old|new</o>`), redacts (`<r>..</r>`) and can type a
  `/Memo << /To .. /From .. /Subject .. /Date .. >>` header and
  `/PageNumbers /bottom`. It leaves the carriage on the next line. `tw-linecount`
  and `tw-pageheight` size a page to its text without drawing. It is a Type 3
  face, so `charpath` gives nothing: use `/CourierPrime-Regular` for outlines.
  From a text file: `pscat typewrite`.
- Released-document look (`lib/released.ps`, after artkit and surfacekit, and
  handscript for notes): `{ content } << /Age 0.6 /EdgeDark 0.5 /Folds 0.5
  /Skew 0.3 /Punch 3 /Generation 3 /Stamps [ << /Lines [ (RECEIVED) ] /X .. /Y ..
  >> ] /Notes [ .. ] >> rel-page` wraps `tw-type` content in aged paper and photocopy
  generations (1 a copy, 3 a copy of a copy of a copy); `<r>word</r>` in the text
  plus `/Redactor { rel-redact }` blacks it out. `rel-stamp`, `rel-note` and
  `rel-paper` work alone. Invent every stamp and name; no real seals.
- Text as geometry: `(word) true charpath` then fill/stroke/clip
  it — or walk it with `alongpath`.
