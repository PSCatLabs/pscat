# TYPEWRITER.md: typewriter design (issue #210)

This is the design for epic #209: a `/Typewriter` font and `lib/typewriter.ps`,
for documents that look as if they were typed on a real typewriter. The
maintainer approved it with every recommendation in section 10. #211 built
sections 2–5 and 7 with flat ink (`lib/typewriter.ps`,
`notes/211-typewriter-font.md`); its public names are `tw-setup` and
`tw-set`, and the machine seed is `/Machine`.

Read these first:

- `docs/typewriter/REFERENCES.md`, the reference board. Observations T1–T15
  and the measurements table are cited below.
- `docs/typewriter/prototypes/tw_proto.ps`, a working prototype of sections 4–6
  (not library code). `docs/typewriter/prototype_vs_real.png` shows what it
  draws next to a real scan.

The user's guidance for this epic: aim for quality and a library an AI artist
can make pieces with; performance is a guard, not a goal; and "we can make
any changes after AI artists use this to make paintings". So this design
makes each call once, from the references, and leaves fine-tuning to #211,
#212 and #215.

## 1. What the references say

| | Mechanism | Evidence |
|---|---|---|
| M1 | **Position noise is small.** Per strike it is 0.15–0.3pt sd in each axis. Per letter, a machine's fixed offsets are about 0.07–0.11pt sd, with a few letters at 0.15–0.25pt. A few percent of strikes land 0.5–1pt off. A line tilts about 0.1° (sd). | the measurements table |
| M2 | **Weight varies more than position.** Ink coverage varies 7–21% per strike within one letter. A rare strike is very light. Some keys print heavy every time. | T2, T7; coverage CV |
| M3 | **The ink spreads past the type face.** Inked stems are 0.9–1.44pt where the face's stem is 0.7–0.9pt. Corners round off. | T2, T4; NBS C431 |
| M4 | **The ribbon's fabric shows.** Ink is mottled at about 0.5pt: the yarn pitch of a plain weave at about 150 yarns per inch. A dry or coarse ribbon breaks strokes into dots. | T4; NBS C431 |
| M5 | **Counters fill, by machine.** Dirt packs one slug's counter, so that letter is filled almost every time it is struck. | T4, T6 |
| M6 | **Ink is not black.** Mean darkness is 0.63–0.81. A worn ribbon is lighter and grey-brown, not just patchier. | T8, T10 |
| M7 | **One side can print heavier**, or half a letter can drop out. | T12 |
| M8 | **Corrections are second strikes at the same cell**: strikeovers, underscores typed after backspacing, x-outs. Erasures leave a ghost. | T3, T5, T11, T13 |
| M9 | **Release copies degrade everything**: filled counters, dropped hairlines, specks. This is a layer on top, for #214. | T15 |

The headline: **most of the look is ink (M2–M7), not jitter (M1).** At body
size, real displacements are a fraction of a point. A fake that exaggerates
jitter to make it visible (the "ransom note" look) is the most common way
typewriter fonts fail. The defaults below follow the measurements. Artists
can raise one knob (`/Wear`) when they want a more battered machine.

## 2. Base face: Courier Prime outlines, embedded

**Decision:** the glyph outlines come from **Courier Prime** (SIL OFL 1.1,
with no Reserved Font Name). A script converts printable ASCII into
PostScript path data, which is committed as a generated file with an OFL
header (`lib/typewriter-outlines.ps`, say). `/Typewriter` draws those paths
itself. It never calls `findfont` on a base face.

`docs/typewriter/base_face_compare.png` sets each candidate at a real scan's
pitch, next to the scan:

| Candidate | Verdict |
|---|---|
| Liberation Mono (pscat's `/Courier`) | **No.** Its letterforms lean towards the sans-serif Cousine design, and its x-height (0.53 em) is large for a typewriter. It is also not what gs uses. |
| Nimbus Mono (gs's `/Courier`) | **No.** Hairline weight (a stem of 0.051 em), and pscat doesn't have it. Embedding outlines is the only way the two interpreters can draw the same face. |
| Special Elite (`fonts/catalog/SpecialElite`) | **No, for body text.** Its wear is baked into each glyph, so every repeat of a letter carries the same damage. That is the fixed-repeat tell the per-strike model exists to remove. Its advance (698 units) is not Courier's. It remains fine for display headings. |
| Cutive Mono (OFL) | **No.** Hairline weight, and a small x-height (0.38 em). |
| **Courier Prime** (OFL) | **Yes.** Slab serifs and a weight close to real pica and elite (stem 0.073 em, which is 0.9pt at 12.5pt). Courier metrics (a 600-unit advance). Built as a typewriter Courier. |

Embedding the outlines (rather than `findfont`) is decided by three facts:

- `/Courier` is a different face in pscat and in gs (above).
- A catalog TrueType face is not found by gs unless its font path is set.
- With the path data in hand, the library knows each glyph's outer contours
  and counters separately. Filled counters (M5) become a simple fill of the
  counter subpaths.

The generated file is 90 KB. It is derived font software, so it carries the OFL
notice and the Courier Prime copyright. #211 also adds `CourierPrime-Regular.ttf`
and `OFL.txt` to `fonts/catalog/CourierPrime/`: that gives the generator a
reproducible input, and gives `/CourierPrime findfont` to anyone who wants the
clean face. `docs/typewriter/prototypes/gen_glyphs.py` is the prototype generator.

## 3. Pitch and size

**Both pitches.** Pica (10 characters per inch, 7.2pt advance) and elite
(12 cpi, 6pt) are both common on the board (T1 is pica; T3–T11 are elite). The
default is pica, which is the more legible at screen sizes.

The type size and the pitch are separate keys, because real elite type is
large for its pitch. The measured cap heights are 7.7–7.8pt on elite and
8.2pt on pica, including about 0.4pt of ink spread. Scaling Courier Prime so
its advance fits 6pt would give 5.8pt caps. So:

- `/Pitch` (`[cpi]`, 10 or 12) sets the advance.
- `/Size` (`[points]`, the em) sets the glyph height.
- The glyphs are scaled horizontally so the advance comes out to the pitch:
  x-scale = (72 / Pitch) / (0.6 × Size).

Starting values are pica at `/Size 12.5` (x-scale 0.96) and elite at
`/Size 11.5` (x-scale 0.87). #211 tunes both against T2 and T10. Line spacing
is 1/6 inch (12pt) per line, and the layout API (#213) gives single, one-and-a-half,
double and triple spacing as multiples of it.

## 4. Font architecture

`/Typewriter` is a Type 3 font (prototype: `tw_proto.ps`, which renders the
same in pscat and gs):

- **`BuildChar` uses `setcharwidth`, never `setcachedevice`.** gs caches a
  glyph drawn under `setcachedevice` and replays the first strike for every
  later one. A probe drew `eee`, each offset by a hash of its position: gs
  rendered three identical boxes under `setcachedevice` and three different
  ones under `setcharwidth`. pscat, which has no Type 3 glyph cache, rendered
  three different ones under both. A cached glyph is also
  a mask, so it would lose the clip-based ink of section 6.
- **`BuildChar` is a pure function** of the font's settings, the cell, the
  code, and the strike index. It never changes a counter. `stringwidth` and
  `charpath` can run `BuildChar`, and must not change what a later `show`
  draws.
- **State lives in a writable side dict.** gs makes a font dict read-only at
  `definefont`, so settings such as the machine seed and the current strike
  index sit in a separate dict that `BuildChar` reads. `lib/fonts/neon.ps` and
  `lib/handscript.ps` use this same pattern.
- **Position is a cell.** `BuildChar` maps its device origin back to the
  text block's frame and rounds to a line and a column on the pitch grid. It
  uses `0 0 transform`, then the inverse of a matrix the library captures
  **when a text block starts** (`twsetup`, and each block in #213's layout
  API), not once at load. An artist who rotates or scales before typing (an
  angled memo), or #214 applying scan skew, then still gets whole lines on one
  grid. The prototype captures it once at load, which is enough for its flat
  page. The cell is the key to all
  per-strike randomness (section 7). Rounding to whole cells means a sub-pixel
  difference between pscat and gs never re-rolls a strike. Text placed off the
  grid still works: it lands in the nearest cell.
- The library offers plain use too. `/Typewriter findfont 12 scalefont setfont`
  then `show` works with the defaults. The layout API (#213) wraps it.

## 5. Variation model

Three layers, all keyed hashes (section 7), never sequential draws. In the
table, `pt` is at the default size; the values scale with `/Size`.

**Machine layer**, fixed for a machine (`/Machine`, an integer seed). For each
printable code:

| Quirk | Default | From |
|---|---|---|
| offset x, y | normal, sd 0.08pt | M1: 0.07–0.11pt per letter after face shape is removed |
| bent slug | 6% of keys get an extra ±0.1–0.25pt vertical offset and a ±0.3–0.8° lean | The offset matches M1's largest per-letter offsets (0.15–0.23pt). The lean is **not measured** (rotation is below the board's resolution). It is a small value set by eye: #211 checks it against T2 and T9. |
| weight | multiplier, normal, mean 1, sd 0.06 | M2: some keys heavy every time (T2's `r`) |
| clogged counter | 15% of letters that have a counter (`a e o` twice as likely); fill fraction 0.5–1 | M5 (T6) |
| one side heavy | 8% of keys; a direction and a strength of 0.3–0.6 | M7 (T12) |

**Line layer**, keyed by the document seed (`/Seed`) and the line index:

| Effect | Default | From |
|---|---|---|
| line-start offset (carriage return) | normal, sd 0.2pt, applied to the whole line | 0.17–0.38pt |
| line advance error | a smooth noise in the line index, amplitude 0.3pt (bounded: not a random walk) | 0.16–0.43pt sd |
| line tilt | normal, sd 0.08° | 0.06–0.10° |

**Strike layer**, keyed by `/Seed`, line, column, code and strike index:

| Effect | Default | From |
|---|---|---|
| offset x | normal, sd 0.2pt | 0.22–0.27pt within a letter (upper bound) |
| offset y | normal, sd 0.15pt | 0.14–0.20pt |
| rotation | normal, sd 0.25° | upper bound only (REFERENCES.md, Rotation) |
| mis-strike | 1% of strikes take 4× the offset above | the tails: 0.8–0.9% of strikes land more than 1pt off horizontally, which a normal of sd 0.2pt almost never gives |
| pressure | normal, mean 1, sd 0.08, times a finger factor (0.9 on the little-finger keys `q a z p` and `; / -`); 1% of strikes are very light (0.5) | coverage CV 0.07–0.21; T7 |

**One knob, `/Wear`** (`[ratio]`, default 1), multiplies every machine and
strike magnitude above. 0 is a perfect machine, 1 is the measured
good-condition manual (Szilard, T9), and about 1.5 is an old, hard-used machine
(Duncan: its sds are 1.2–1.5× Szilard's, and strikes 0.5pt or more off the
baseline are 8× as common). Values above 2 are for art. Presets set several
keys together:

| `/Preset` | Settings |
|---|---|
| `/manual` (default) | `/Wear 1`, pressure sd 0.08, `/Ribbon 0.7` |
| `/electric` | `/Wear 0.7`, pressure sd 0.03 and no finger factor (M2; T11's evenness) |
| `/worn` | `/Wear 1.5`, `/Ribbon 0.35` |

**Strike index.** A second strike at the same cell needs its own variation (T3,
T5). The side dict holds a strike index, 0 by default, which is part of the
strike key. The layout API bumps it for strikeovers, x-outs, bold by
double-striking, and the underscore pass. `show` itself never bumps it, because
`BuildChar` must stay pure.

## 6. Ink model

All of it is vector. Each strike draws, inside one `gsave`:

1. **Weave clip (M4).** Clip to the ribbon's inked area: a union of rectangles
   (`rectclip` with an array operand: one operator, any number of rectangles)
   that leaves holes at the yarn pitch. The pitch is 0.48pt in absolute size: the
   ribbon doesn't scale with the type. The library precomputes 8 tiles and picks
   one per strike at a random sub-yarn offset. The hole probability and size grow
   as the ribbon dries (`/Ribbon` falls) and as pressure falls. Holes are a clip,
   not paper-coloured paint, so whatever paper or stain #214 has painted
   underneath shows through them. The prototype's holes sit on a regular grid
   and read as a halftone. #212 must jitter their size and position per yarn.
2. **Body.** Fill the glyph (outer contours and counters together, so the
   nonzero rule keeps counters open) in the ink colour. **No step paints a
   colour lightened toward the paper.** Pressure and `/Ribbon` act only on
   coverage (the hole tiles, step 1) and on spread (step 3). A worn or faded
   ribbon is expressed in `/Ink` itself (T10's grey-brown). So the library
   never needs to know the paper's colour, and #214 can tone or stain the
   paper freely.
3. **Spread (M3).** Stroke the same outline with a round join. The width is
   set by pressure × machine weight × `/Ribbon`, about 0.25–0.6pt in total
   (0.12–0.3pt per side). This brings the face's stem up to the measured
   0.9–1.44pt and rounds the corners. Because it is stroked through the weave
   clip, the edge breaks up where a hole falls on it: the feathering.
4. **Filled counter (M5).** For a clogged key, fill the counter subpaths at
   the clog fraction × pressure.
5. **One side heavy (M7).** For a one-side key, draw the body through a
   sparser weave tile (more holes), then draw it again through the normal tile,
   clipped to a half-plane in the key's direction. Both passes use the same
   ink: the light side is lighter by coverage, not by colour.

Keys: `/Ink` (`[rgb array]`, default near-black `[0.13 0.13 0.15]`: M6; a
purple or blue-black ribbon is just a colour) and `/Ribbon` (`[0..1]`, wetness,
default 0.7). A red/black two-colour ribbon is `/Ink` switched per run. Bleed
between the two halves of the ribbon is deferred (Q3).

**gs fill adjust.** The weave holes are 1–2 device pixels at scan resolution,
which is exactly what gs's default fill adjust closes up (DRYMEDIA.md 2.4). The
library calls `/.setfilladjust2 where { pop 0 0 .setfilladjust2 } if` inside its
own `gsave`, as drykit does. pscat skips it.

Carbon copies, photocopy loss, stamps and redactions belong to #214, on top of
this.

## 7. Randomness that stays out of the caller's stream

- The library never calls `rand` or `srand`.
- Every random value is a hash of integer keys through a private 256-entry
  permutation table. The table is shuffled **once, from a fixed constant**, by
  the Schrage-form LCG that paintkit uses for `pjrand` (CANVAS_SURFACE.md 3.4).
  The seeds are never used to shuffle it. They enter as keys:
  - machine quirks hash `/Machine`, the code, and a slot;
  - line effects hash `/Seed`, the line, and a slot;
  - strikes hash `/Seed`, the line, the column, the code, the strike index, and a
    slot.

  So a new document (`/Seed`) never re-rolls the machine's quirks: the same
  machine's `e` sits high in every document, the epic's principle.
- **Each key is its own hash input.** Keys are folded into the hash state 8
  bits at a time, so seeds and columns above 255 stay distinct, not truncated
  to 256 values. The slot number (which quantity: x offset, y offset, pressure,
  ...) is a separate key too. Adding it to the column, as an early draft of the
  prototype did, makes column c+1's x equal column c's y, and strike index 1 at
  column c equal strike index 0 at column c+31. Those collisions hit exactly the
  overtype runs that the strike index exists for. `tw_proto.ps`'s `mix` and
  `hn` show the shape.
- An approximately normal value is the sum of three hashed uniforms.
- Consequences:
  - The caller's `rand` stream is untouched, so `--sweep-seed` varies a piece's
    composition while its typing stays fixed.
  - Same seeds, same page, in pscat and gs: integer hashing has no
    floating-point drift between the two.
  - Typing text in one place never re-rolls another line, because nothing is
    drawn from a sequence.
  - `stringwidth` and `charpath` are side-effect free.
- The prototype's hash has 8-bit outputs. That is enough for jitter. #211 may
  fold one more round for 16 bits where a smooth value is needed (the
  line-advance noise).

## 8. Performance budget

**Measured** with `tw_proto.ps`, MODE 1: a full page of 50 lines × 50 strikes
(2,500 cells, about 2,100 glyphs, the rest spaces), at 300 dpi, with Courier
Prime outlines. These are CPU times (`user`), best of 3. pscat is this branch's
release build (`target/release/pscat` at the #210 commit); gs is 10.07.1. The
load average was high (9–18, from other agents), so treat these as upper bounds.

| Case | pscat | gs |
|---|---|---|
| outlines and jitter, no ink | 0.32s | 0.20s |
| with the ink model (weave clip, fill, spread stroke, counters) | 1.00s | 0.44s |

An earlier draft of the prototype, with a cheaper but collision-prone hash
(section 7), measured 0.64s and 0.28s with ink. The hash is now about half
the cost, and #211 can cache per-machine values in a 95-entry table.

**Budget (advisory, not a CI gate):** a full page of about 2,500 strikes
renders in **5s of CPU or less** in each interpreter at 300 dpi. That is about
5× the prototype in pscat and 11× in gs, so #212 has room for a richer ink
model. If #212's ink goes over it, simplify the ink or raise the budget. The
budget is a guard against an accidental 10× regression, not a target to
optimize toward.

**In CI**, tests render a strip of a few lines and use the existing
`within_cpu_budget` 2s cap (`tests/common`; AGENTS.md's code quality bar).
No new page-scale CI test is added. #204's concern about CI time stands.

## 9. What each child ticket builds

- **#211, core font.** The outline generator and `fonts/catalog/CourierPrime`;
  the `/Typewriter` Type 3 font with the side dict, the cell mapping and the
  hash; the machine, line and strike layers with `/Wear` and the presets; plain
  flat ink. It starts with the feasibility check the epic asks for, which the
  prototype has already passed in both interpreters. Tests: a determinism test
  (same seeds give the same pixels); a check that two strikes at one cell with
  different strike indices differ; a check that `stringwidth` leaves the page
  unchanged; a check that the caller's `rand` stream is untouched; and a gs run.
- **#212, ribbon ink.** Section 6, tuned against T2, T4, T6, T8 and T10 at
  scan resolution.
- **#213, layout.** `tw-type` with margins, tab stops, line spacing,
  strikeover, x-out, underscore and erasure ghost (each by bumping the strike
  index), memo templates, `scripts/typewrite.sh`, the capability catalog, and
  the pscat and psart skill pages. Prefix note: artkit already uses `tw`-prefixed
  names for its text wrapping (`twgn`, `twgrem`, `twline`, `twlines`, `twlast`,
  `twn`, `twrest`, `twstr`), so the library's public names must avoid those, and
  its internals live in the side dict.
- **#214, released-document look.** T14 and T15: aging, photocopy and bitonal
  loss, stamps, redactions. Everything fictional.
- **#215, specimen.** A side-by-side against T1/T2 and T9/T10 crops at scan
  resolution. The user judges it.

## 10. Open questions for the maintainer

Each is phrased as a decision, with a recommendation.

- **Q1. Courier Prime as the base, embedded as generated outlines?**
  Recommend yes (section 2). This adds about 90 KB of OFL-licensed generated
  path data under `lib/`, and the Courier Prime TTF to `fonts/catalog/`.
- **Q2. Monospace only?** Proportional executive faces (T13, T14; IBM
  Executive) were common in 1950s–60s government offices. Recommend leaving
  them out for now. They need proportional metrics and a different face, and
  the epic's target is the typed memo.
- **Q3. The unsourced traits:** two-colour ribbon bleed, punctuation embossing,
  and `xxxx` overtyping. Recommend:
  - two-colour ribbon: a plain `/Ink` colour switch now, with bleed deferred
    until a reference turns up;
  - embossing: treat it as extra pressure on `. , ' "` (a darker, slightly
    larger dot), not as a lighting effect;
  - x-outs: build them from strikeovers, since they are the same mechanism;
  - electric typewriters: the board has no confirmed electric sample. The
    O'Brien memo is electric only by inference (REFERENCES.md, "Electric, by
    inference"). The `/electric` preset is a lower-variation manual, not a
    separate model.
  #215's agent looks for references to two-colour bleed, embossing and a
  confirmed electric (IBM Selectric) page before the comparison.
- **Q4. Defaults that follow the measurements.** At body size and screen
  resolution, measured jitter is barely visible, and the default page may look
  "too clean" at first glance. Recommend keeping the measured defaults
  (`/Wear 1`), and letting the specimen and artists raise `/Wear`. The ink
  carries the look.
- **Q5. The performance budget.** Recommend 5s of CPU per full page in each
  interpreter, advisory only, with CI rendering strips under the existing 2s
  cap (section 8).
