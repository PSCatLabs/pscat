# #214: the released-document look

Epic #209. Builds on #211 (font), #212 (ribbon ink) and #213 (layout); read
`notes/213-typewriter-layout.md` first. The design is `docs/TYPEWRITER.md`
(section 9, "As built (#214)"); the reference crops are T9, T10, T14 and T15
in `docs/typewriter/REFERENCES.md`.

## What shipped

- `lib/released.ps` (needs artkit and surfacekit; handscript for notes):
  - `rel-page`: `{ content } opts rel-page`. Paper, scan, copies and marks
    around any drawing.
  - `rel-paper`: the paper alone, for `tw-type`'s `/PageStart`.
  - `rel-stamp`, `rel-note`, `rel-redact`: single marks, usable alone.
- `lib/typewriter.ps`:
  - `/Copies` (tw-setup / tw-set / tw-type): copy loss in the ink.
  - `<r>text</r>` markup and a `/Redactor` hook in `tw-type`.
- `examples/released_specimen.ps`, rendered to `docs/released_specimen.png`
  (pscat, 150 dpi) and `docs/released_specimen_gs.png`; the board
  `docs/typewriter/release_vs_real.png`
  (`docs/typewriter/tools/release_compare.py`) sets crops against T10, T14, T15.
- `tests/released.rs` (9 tests), `%%SelfTest rel-guards` and the
  tw guards, `tests/libbind.rs` and `selftest/libraries.txt` entries.
- Docs: `docs/guides/fonts.md`, `docs/TYPEWRITER.md`, `HANDOFF.md`, the pscat
  and psart skill pages. The catalog rows come from the `@kind` tags.

## Acceptance criteria

1. **Each effect is optional and has a strength.** `/Age /EdgeDark /Folds
   /Lamp /Skew /Generation /Speckle /CopyEdge /Punch /Staple /Clip /Shadow
   /Redact`, stamps' `/Strength`. `rel-page` with no option runs the content
   and draws nothing (the test compares pixels), and each key at 0 equals the
   plain page.
2. **Deterministic under the seed.** The library has its own generator
   (Schrage's minimal standard, `rlU`), so a seed gives the same page in
   pscat and gs and the caller's `rand` stream is untouched (tested). The paper
   grain and fibres are surfacekit's, which uses `rand` under its own restored
   `/Seed`; those are the only marks that differ in detail between pscat and gs.
3. **Specimen at 0, 1 and 3 generations** with stamps, redactions and a margin
   note: `docs/released_specimen.png`.
4. **pscat and Ghostscript.** The specimen is committed from both; a test runs
   gs on it (local only) and compares each sheet's mean brightness (paper
   tones match to the byte at sampled points).
5. **Catalog entries**: `rel-page`, `rel-paper`, `rel-stamp`, `rel-note`,
   `rel-redact` through the `@kind` tags. No `% @param` without a unit.
6. **Typewriter renders unchanged.** `scripts/identity_check.sh --loader
   lib/typewriter.ps` is identical for `examples/typewriter_specimen.ps` and
   its control (a driver that draws only if `/Copies` is a known key) differs;
   `scripts/typewrite.sh` renders the memo byte for byte the same on both trees.

## Decisions made here

- **No alpha, no read-back.** Ghostscript has no alpha and PostScript cannot
  read what it drew, so there is no post-filter. The effects are:
  - *under* the content: paper as a field. The sheet is cut into cells, 1.5pt
    at the edge growing to 9pt inside, each one opaque fill of the colour the
    paper has at its centre: tone, shifted toward a stain by 3 to 9 elliptical
    blotches (a sum of Gaussians), shaded by the lamp, darkened by the edge
    term. Stepped rings overpainting each other (the first attempt) showed
    hard edges as soon as the lamp was on, because each ring knew nothing of
    the others.
  - *copies* as a chain of clips: sheet *k* is clipped into the frame of sheet
    *k+1* by a small rotation and shift (up to 3pt and 0.3 degrees), so the
    content carries the sum of them and each copy's specks are tilted again by
    the next. Where a copy's image does not reach its sheet, the copier's lid
    shows (an `eofill` of the sheet minus the transformed last sheet), with a
    short ring-stepped shadow along the image's edge.
  - *copier curve*: paper tones and stains go white and dark things stay dark,
    applied to every colour the library sets (`rlSet`). It acts on darkness
    d: `S = clamp((d - 0.4)(1 + 2.5 c) + 0.45)`, blended in over c up to 0.33.
    c is `1 - 0.6^n` for the n copies still to come.
  - *marks over the type* in the original's frame, so a copy copies them.
  - *stamp ink*: a clip of the stamp's box with holes (a hole field denser
    toward one side, and a few voids), so the page and the type under the
    stamp show where the stamp is light. Not paper-coloured paint.
- **Copy loss in the typewriter, not a post-filter.** A filled counter or a
  dropped hairline cannot be added from outside `BuildChar`. `/Copies` n
  (c = `1 - 0.55^n`, 0 when unset: the branch does not run, so existing renders
  are byte-identical) adds spread (`+0.7c`), more weave holes for light strikes
  (a contrast on the hole level, `+0.14c`), and fills every counter by `0.8c`.
  `rel-page` sets it to `/Generation` for the content unless the content sets
  its own.
- **Redaction is markup.** `<r>word</r>` marks cells (attribute bit 4), so a bar
  always covers whole words and the layout never needs a coordinate. The bar is
  drawn after each line's passes; a run is one bar per line. The hook is called
  as `x0 x1 baseline size` with `TWState` ended (as `/PageStart` is). Without
  it a plain black bar is drawn. `rel-redact` is the marker's: wavy top and
  bottom, cut corners, a slight tilt, a second streaky pass.
- **One page.** `rel-page` wraps one drawing. Overlays on pages 2 and later
  would need a `/PageEnd` hook (a `showpage` resets the CTM); `rel-paper` as
  `/PageStart` ages every page and the other layers are for one page.
- **Stamps are generic.** `/Lines`, `/Font`, `/Tracking`, `/Frame`; no
  built-in text. The specimen's are "RECEIVED 14 MAR 1964" and "RELEASED IN
  PART". The notes and names are invented. No agency seals or markings.
- **Gotchas found.**
  - `def` inside RelState on a procedure value: fetch it with `load`, as
    `/PageStart`.
  - A guard that fails before the dictionary is ended leaves it on the stack:
    a standalone `rel-stamp` that read `rlW` before any `rel-page` did exactly
    that (fixed by running `rlDefaults` at load, and a test).
  - gs needs `-g` for the specimen (the BoundingBox is not a page size).

## Against the references

`docs/typewriter/release_vs_real.png`:

- Paper (T10): tone, mottling and the darker margin are in range; the real scan
  is greyer and its ink softer (that is an `/Ink` and `/Ribbon` choice, #212).
- Copy loss (T15): specks, closed counters and heavy strokes read as the real
  thing at gen 3. The real copy also loses thin strokes to dropout and has
  uneven density along a line; ours thickens more than it breaks.
- Marks (T14): the redaction bar and stamp read well. Real stamps are
  smaller and lighter, with a softer edge from the copier.

## Not done (filed, all low priority)

- #448: multi-page overlays (a `/PageEnd` hook). `rel-page` is one page: the
  content must fit the sheet and not `showpage`. If the content raises an
  error the typewriter's `/Copies` stays set.
- #449: thin-stroke dropout under a copy (T15).
- #450: copier softening of stamps, notes and bars (T14).
- #451: a paper clip that reads less like three ellipses.
- `rel-note` depends on `hs-write`'s `/Pen`, `/Jitter` and `/StartWobble`
  (lib/handscript.ps, which #416 is changing): recheck after it lands.
