# #212: ribbon ink

Epic #209. This ticket builds section 6 of the design in `docs/TYPEWRITER.md`,
on top of #211's `/Typewriter` font. Read `notes/211-typewriter-font.md`
first. TYPEWRITER.md's section 6 now ends with an "As built" list.

## What shipped

- `lib/typewriter.ps`, section "ribbon ink":
  - `tInkStrike` draws one strike: a feather pass, then the body, then
    clogged counters (`tClogBand`).
  - A one-side-heavy key draws its body twice: through a sparser weave, then
    through the normal one on its heavy half.
  - The weave bank `tWeave` is 4 variants × 16 levels of hole paths. It is
    built once at load.
  - `tGBox` and `tCBox` hold each code's outline and counter bounding boxes.
- `tStrikeGlyph` now keeps the machine table's clog and one-side values and
  the pressure `tPr`. It calls `tInkStrike` when `/InkModel` is true. False
  runs #211's code unchanged.
- A new key, `/InkModel` [boolean], default true. It is on `tw-setup` and
  `tw-set`, with a guard and a `%%SelfTest` line.
- `docs/typewriter/tools/ink_compare.py` builds
  `docs/typewriter/ink_vs_real.png`. It types the text of T2, T4, T10, T6 and
  T7 at each scan's own resolution, in pscat and gs, scaled by that crop's
  factor from `make_board.py`.
- The specimen (`examples/typewriter_specimen.ps`) is re-rendered at 200 dpi,
  in pscat and gs:
  - row 1 is flat ink (`/InkModel false`) and rows 2–4 are ribbon ink;
  - the third repeats row is machine 7's clogged `o`;
  - the presets gain a bone-dry ribbon line (`/Ribbon 0.1`).
  - #420 is folded in: the preset sentence is shortened so its lines end at
    the right rule.

## Acceptance criteria

1. **Reference comparison at scan resolution.** See
   `docs/typewriter/ink_vs_real.png`: T2 and T4 at 300 dpi, T10 at 240 dpi, and
   the T6/T7 letter tiles. Each panel has three rows: scan, pscat, gs. Tuning
   took four rounds by eye:
   - Round 1 was much too bold. The feather was added on top of #211's full
     spread.
   - Round 2 split the spread and added hole clumping, so the voids gather
     into blotches instead of an even pin-prick grid.
   - Round 3 thinned the body stroke.
   - Round 4 made the hole size and density steeper as the ribbon dries.

   Still visibly different from the scans, and filed as #423:
   - The renders are a little heavier than T2 and T4. Part of that is the
     Courier Prime face itself: elite stems are 0.84pt before any ink.
   - Real repeated strikes vary in weight more than ours do (T7 has one
     nearly blank `e`).

   The maintainer judges the renders before merge.
2. **Determinism.** Every ink draw is a hash: the strike prefix, the machine
   table, the document seed for the wear drift, and a fixed constant for the
   bank.
   - `the_same_seeds_type_the_same_page_byte_for_byte` and
     `the_callers_rand_stream_is_untouched` now run with the ink on, since it
     is the default.
   - The new slots are: domains 8 and 9 (the bank), document key 4 (the wear
     drift), strike sub-key 20 (the weave frame), and machine slot 14 (the
     clog direction). None reuses a #211 slot.
3. **Budget.** I timed only pscat, as a guard (CPU, best of 2, load average
   about 5). TYPEWRITER.md section 8's full page (50 × 50 cells at 300 dpi)
   took **1.9 s user + 0.9–1.1 s sys** with ink, and 0.22 s flat. That is
   within the 5 s advisory budget. Ghostscript is a correctness check only
   here: the specimen, the comparison and the gs test render without error.
   It was not timed against the budget. The CI strip test
   (`a_strip_types_within_the_cpu_budget`) runs with the ink on and passes.
4. **Ink off is #211, byte for byte.**
   - Driver 1 is the #211 specimen (base tree's copy). Driver 2 is a mixed
     page: `/Wear 2`, worn elite, and plain `findfont` at 12pt.
   - Both drivers set `/InkModel false` only when the key exists
     (`TWState /tKeys get /InkModel known`).
   - Each driver was rendered in the merge-base tree and in this branch with
     one binary. Both pairs are identical by `cmp`.
   - Control: driver 2 with the ink on differs.
   - `scripts/identity_check.sh` was not used: its file pick (`--loader`)
     finds only the specimen, which now draws with the ink on.

## Decisions made here

- **eoclip, not a rectclip union.** One outer rectangle plus non-overlapping
  holes, filled by the even-odd rule, is the same clip. It also lets the holes
  be jittered quadrilaterals. The spike rendered at 200, 300 and 600 dpi in
  both interpreters.
  - gs clips without anti-aliasing, so its holes are crisper at 200 dpi.
  - `.setfilladjust2 0 0` already runs before the clip is built.
- **Holes are nested across levels.** Each yarn cell has one rank. A level
  draws the holes whose rank is below it. So a heavier strike's holes are a
  subset of a lighter one's, and density changes smoothly.
- **Ranks are clumped.** A rank is the mean of the cell's own uniform and a
  2 × 2-cell field's, mapped back to uniform through the triangular CDF. So a
  level's hole fraction still equals the level.
- **The bank is in points.** The yarn pitch is 0.48pt whatever the type
  size. A strike scales the bank into glyph units by `advU / cellW`.
- **The spread is split** between a solid body (0.3 of #211's width) and a
  feather through a weave sparser by 0.5 (0.85 of the width). With the
  feather added to the full spread, the first round was far too bold.
- **The clog band.** The design says "fill the counters at the clog fraction
  × pressure". Here that fraction is the share of the counter's
  circumscribed circle that a band covers. The band starts from a fixed side
  per key, since dirt packs one side. Its edge is ragged: solid to 0.12 short
  of the fraction, then sparse to 0.12 past it. Machine wear scales the
  fraction, so `/Wear 0` has no clogs. A first version sized the band in fixed
  units, which filled nearly every clogged counter whole.
- **No feather at `/Ribbon 0`.** Like #211's spread, a 0-width feather stroke
  would be a 1-pixel hairline around every glyph, so it is skipped.
- **One side heavy, checked by eye.** Machine 3's `R` (strength 0.46,
  angle 173°) at `/Size 40` has a dense left half and a patchier right one.
  At `/MachineWear 0` it is even. The boundary is a straight line through
  the glyph's box centre. That is visible only when enlarged.
- **`/StrikeWear 0` keys the ink by the key.** The weave's angle, offset and
  variants come from the machine table's hash, and the wear drift is off. A
  quirks-only page then repeats each key exactly, ink included, so #211's
  quirks test and specimen row 2 keep their meaning.
- **Rounding tolerance in tests.** The same strike at another cell rounds 1–2
  pixels differently: hundreds of clip edges at another device origin. A
  `show` that reaches column 3 by adding advances does the same, against a
  `moveto`. Four comparisons that were exact under flat ink now use
  `nearly_same` (at most 12 pixels differ in a cell). A different strike moves
  hundreds of pixels. Same-position determinism stays exact.
- **Not reused: surfacekit, stipplekit, artkit `scatter`.**
  - All three draw from `rand`, which would break "independent of the
    caller's rand stream".
  - They place marks in page space, but the ink must be a function of the
    cell inside a pure `BuildChar`.
  - The weave bank is the one placement structure here. It is a clip, not
    marks.

## Follow-ups

- #422: the worn ribbon band and the two-colour bleed. Both are optional in
  the issue, and the design defers the bleed.
- #423: the ink is a little heavy against T2 and T4, and the strike weight
  is too even against T7.
