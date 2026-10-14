# #211: the /Typewriter font

Epic #209. The design is `docs/TYPEWRITER.md` (#210), which the maintainer
approved with every recommendation in its section 10. This ticket built
sections 2–5 and 7 of it, with flat ink. Section 6 (ribbon ink) is #212.

## What shipped

- `fonts/catalog/CourierPrime/`: `CourierPrime-Regular.ttf` and `OFL.txt`
  from `github.com/google/fonts` (`ofl/courierprime`; sha256
  `72f79337…b978af`). `/CourierPrime-Regular findfont` now gives the clean
  face.
- `scripts/gen_typewriter_outlines.py` writes `lib/typewriter_outlines.ps`
  (92 KB, OFL header): `TWOutlines code [ {outer} {counters} ]` for 32–126,
  in 1000 units per em. It replaces the prototype generator
  (`docs/typewriter/prototypes/gen_glyphs.py`, kept as the #210 record).
  Outer contours and counters are told apart by winding. The procedures are
  bound as they are read. The script checks every advance is 600 units.
- `lib/typewriter.ps`:
  - the `/Typewriter` Type 3 font;
  - the `TWState` side dict;
  - `tw-setup`, which starts a block: it applies the options, captures the
    cell frame and sets the font;
  - `tw-set`, which changes options inside a block without moving the
    frame.
- `examples/typewriter_specimen.ps` renders `docs/typewriter_specimen.png`
  (pscat) and `docs/typewriter_specimen_gs.png` (gs). Both are 200 dpi.
- `tests/typewriter.rs` (8 tests), the `%%SelfTest` blocks in
  `lib/typewriter.ps` (guards, presets, pica and elite advances), and the
  library added to `tests/libbind.rs`.

## Acceptance criteria

1. **Feasibility, in both interpreters.**
   - The design replaced "BuildGlyph selects the TrueType base face and
     charpaths it" with embedded outlines. The reasons are in TYPEWRITER.md
     section 2: `/Courier` is a different face in pscat and gs, and gs can't
     find a catalog TTF without a font path. So no `findfont` happens inside
     `BuildChar`.
   - What has to work in both interpreters is a Type 3 `BuildChar` that
     draws path data under `setcharwidth`, and gs not caching its glyphs.
     `ghostscript_types_the_same_page_and_caches_no_glyph` proves both: in
     gs, 21 strikes of `e` are pairwise different.
   - **Accepted limit:** `charpath` on a Type 3 font captures no outline
     (FONTS.md Decision 6, and `lib/lettering.ps`'s header). Other
     libraries cannot outline `/Typewriter`. Use `/CourierPrime-Regular`
     when you need outlines.
   - No fallback to a `show` replacement was needed.
2. **Repeated strikes differ; quirks hold.**
   `repeated_strikes_differ_but_a_machines_quirks_hold`:
   - With strikes on, 21 strikes of `e` on 3 lines are pairwise different.
   - With line and strike variation off, all 21 are pixel-identical.
   - They are identical again under another `/Seed`, and different under
     another `/Machine`.
3. **Determinism.** `the_same_seeds_type_the_same_page_byte_for_byte`
   checks that the same seeds give the same page byte for byte, and that
   a new `/Seed` or `/Machine` changes it. It also covers plain
   `findfont`/`show`. The issue says `/MachineSeed`; the approved design
   (section 5) calls it `/Machine`.
4. **Zero variation is the plain face.**
   `zero_variation_is_the_plain_courier_prime_face`:
   - The test types at `/Wear 0 /Ribbon 0`. `/Ribbon 0` turns off the ink
     spread, which is ink, not variation.
   - It compares the result with the TTF drawn through pscat's own font
     machinery, each character placed on the pica grid. The mean pixel
     difference is 0.011 (of 255). With the spread on it is 1.2.
   - The TTF's own advance is 1228/2048 em, 0.07% short of 600 units. A
     whole-line `show` drifts by that much, so the reference places each
     character itself.
   - If #212 gives `/Ribbon 0` another meaning (no ink at all), this test
     needs another way to turn the spread off.
5. **Specimen.** `docs/typewriter_specimen.png` has four sections:
   - the full ASCII set four times: `/Wear 0`, quirks only, `/Wear 1`, and
     `/Wear 2.5`;
   - enlarged repeats of `e` and `a`;
   - the presets, elite pitch, a red ribbon run and a strikeover.
6. **Budget.** The budget is TYPEWRITER.md section 8's: advisory, 5 s of
   CPU for a full page. The measurement:
   - a full page of 50 × 50 = 2,500 cells at 300 dpi, with the defaults;
   - user CPU, best of 3, at a load average of about 15: **0.21 s in pscat
     and 0.13 s in gs 10.07.1**.

   `a_strip_types_within_the_cpu_budget` holds three lines to the 2 s CI
   cap. They take 0.02 s.

## Decisions made here

- **Two procedures.** `tw-setup` starts a block and `tw-set` changes
  options within one.
  - With one procedure, switching `/Ink` for a red run would recapture the
    frame mid-line.
  - Options persist from call to call. `/Preset`, when given, first resets
    `/Wear`, `/Ribbon` and the pressure model.
  - Unknown keys are an error: a misspelt `/wear` would otherwise do
    nothing, silently.
  - Every key is checked before any is applied.
- **Per-layer wear.** `/MachineWear`, `/LineWear` and `/StrikeWear` each
  multiply one layer, on top of `/Wear`. AC5 needs "quirks only", and an
  artist may want a steady hand on a bad machine.
- **The frame.** It is the CTM at `tw-setup`, moved to the current point if
  there is one. Baselines a whole number of lines below that point land on
  integer cell rows, never on a .5 rounding boundary.
- **Cell sizes come from the glyph's actual advance and em** (in the frame),
  not from `/Pitch`. Plain `12 scalefont` use (advance 6.9pt) still gets a
  distinct cell per glyph, rather than two glyphs rounding into one cell
  every 25 columns and sharing their draws.
- **A 16-bit hash state.** The prototype's 8-bit Pearson chain collapsed
  every prefix to 256 states, so one strike in 256 got another cell's
  draws. Two Pearson chains, with different starts and byte offsets, give
  16 bits.
  - Seeds are folded in at `tw-setup`; the cell, the code and the strike
    index are folded in per strike.
  - Separate domain keys keep the machine, line and strike draws apart.
- **The machine table** (95 entries) is rebuilt only when `/Machine`
  changes. The line layer is memoized per line. Both are memoization, so
  `BuildChar` stays pure in effect.
- **Pressure and weight** set the width of the spread stroke:
  `0.6pt × /Ribbon × pressure × weight`. That is section 6 step 3, the one
  part of the ink model that flat ink can show.
  - The stroke is drawn after the `/Pitch` squeeze is undone, so the pen
    is round.
  - At width 0 nothing is stroked: a 0-width stroke is a 1-pixel hairline.
- **Embossing** (Q3) is extra pressure, +0.15, on `. , ' "`. The finger
  factor is −0.1 on `q a z p ; / -` and their shifted keys; `/electric`
  turns it off.
- **gs fill adjust.**
  - `.setfilladjust2 0 0` runs inside each strike when it exists, as
    section 6 asks.
  - Without it, gs's dilation filled elite counters at 200 dpi.
  - pscat has no such operator and skips it.
- **The catalog scan.** build.rs's scan panics on `/x 96 dict def` (only
  `/name value def` is understood) and requires a tag on every top-level
  `def`.
  - The side dict is `<< >>`, filled inside `TWState begin { ... } exec
    end`.
  - The font is built inside `{ ... } exec` too.
  - So the scan sees four names: `TWOutlines`, `TWState` (both
    `@internal`), `tw-setup` and `tw-set`.
- `/Typewriter` is not yet a `Type3Face` row in the capability catalog.
  #213 owns the catalog and skill pages.

## For #212

- The machine table entry is
  `[x y rotation weight-dev clog-fraction one-side-angle one-side-strength]`.
  Clog and one-side are 0 for keys without the quirk.
  - Clog: 15% of keys with a counter, 30% for `a e o`.
  - One side heavy: 8% of keys.
- The weave clip goes around the fill and stroke in `tStrikeGlyph`.
- With the fill adjust off, gs and pscat put down the same amount of ink.
  Over the presets and elite rows of the two specimen PNGs, the mean
  darkness is 23.6 in gs and 23.3 in pscat. Tune the ink against the
  references in both.
