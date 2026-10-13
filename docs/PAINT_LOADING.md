# PAINT_LOADING.md: double-loaded brushes and highlight colours (issue #183)

Design for #183, part of epic #112. Nothing here is implemented yet, and
the maintainer approves it before any code is written. The companion
design is `docs/CANVAS_SURFACE.md` (#182). The two docs share the
vocabulary below and the "interaction" sections check that they don't
collide.

## Vocabulary shared with CANVAS_SURFACE.md

| Term | Meaning |
|---|---|
| **head position** `u` | Where a bristle, lane or striation sits across the brush head: -1 at one edge, +1 at the other, 0 on the centerline. Each preset already computes it: `pfub` (pkfan), `ptnc` (pktrowel) and `pmnc` (pkbroad) directly, and pkdry/pkoil as `2*evenfrac - 1` from `pbevenfrac`/`poevenfrac`. +1 is the **left** of the direction of travel in every one of them, because they all offset along `angle + 90`. |
| **loaded colour** | A colour the brush carries, as opposed to the current colour the caller set. This doc only. |
| **load** | The set of loaded colours plus how they are arranged on the head. Not the key `/Load`, which stays a contact rate (#141). |
| **canvas space** | The user space captured when the canvas surface is set up. CANVAS_SURFACE.md only; this doc uses user space, like `pkdab /Light`. |
| **surface**, **touch**, **catch** | CANVAS_SURFACE.md's terms: the shared tooth, the `/Touch` key, and the clip a light touch builds from the surface. |
| **unset** | A key that is absent from the options dict. Branches test `opts /Key known`, never a default value that happens to compute the same thing (the #181 `/Direction` rule). |

## 1. The problem

Every paintkit preset paints in one colour: whatever `setrgbcolor` set,
varied by `/ColorJitter`, and lightened or darkened with artkit's
`shade` (toward white above 1, toward black below 1). A real brush is
rarely loaded that way, and `gallery/alpine_lake.ps` (#118) shows what
that costs. Line numbers below are from `origin/main` at 0533d48.

- **Pine highlights are a second pass with second geometry.** `lkpine`
  (lines 299-332) paints each branch tier as a dark `pkfan` stroke in
  `lkPINE` (lines 313-319), then paints the lit side again as a separate
  `pkfan` call in `lkPINELT` with a different span (`0.28` vs `0.34`), a
  2pt lift and its own bristle count (lines 321-331). Each tree is about
  2 x (n+1) tier calls plus n/2 highlight calls, for eleven trees, six
  of them painted twice because the reflection re-runs the scene. The highlight
  bristles share nothing with the tier they are meant to light, and the
  render pays for the extra pass. A fan loaded with dark green and a
  touch of yellow-green on one corner would paint both in one stroke
  from one set of bristles.
- **Snow and shadow snow are separate knife pulls.** `lkrange` lays five
  `lkSNOW` pulls down each lit face (lines 205-212), then four separate
  pulls of `[lkrc lkSNOW 0.55 mix3]` down the shaded face (lines
  213-220). A knife loaded with white and a little blue lays a lit edge
  grading into shadow in one pull, with the two colours sharing the
  blade's lanes, scrape streaks and drag.
- **Highlights can only be white-ward.** The bank lip is `pkoil
  /Highlight 0.3` (line 437) and the bushes are `pkdab /Highlight 0.6
  /Shadow 0.5` (lines 442-446). Both highlights are `shade` toward
  white, so a sunlit bush gets grey-green highlights rather than
  yellow-green ones, and its shadows go toward black rather than toward
  a cool blue. `pkdab`'s NOTES entry already defers this: "a distinct
  highlight hue is done by a second pkdab call."
- **Two-colour strokes are faked by alternating strokes.** The brushed
  earth (lines 425-432) picks one of three colours per `pkbroad` stroke
  at random, and the sky's criss-cross (lines 95-103) gives each stroke
  one shade of the gradient at its start height. A broad brush carrying
  brown and green, or zenith blue over horizon gold, would lay both in
  one stroke.

Not every two-tone mark wants this. The mist fade (lines 228-238) is a
vertical gradient across 17 strokes, so it stays a stack of strokes.

## 2. Artist-facing API

### 2.1 Keys

All keys are optional. None of them is `/Load`.

| Key | Unit (catalog text starts with) | Default | Meaning |
|---|---|---|---|
| `/Colors` | `[array of rgb arrays]` | unset | 1 to 8 loaded colours, listed from the head's left edge (looking along the direction of travel) to its right edge, evenly spaced across it. When set, the current colour is not used for the mark. |
| `/ColorMix` | `[0..1]` | 0.3 | How far neighbouring colours have blended on the brush: 0 is hard side-by-side bands (a clean double load), 1 is a smooth gradient from edge to edge. |
| `/Light` | `[degrees]` | unset | With `/Colors`: the direction the light comes from, counterclockwise from +x in user space (`pkdab`'s existing meaning, unchanged). The first colour goes on the side of each subpath that faces the light, whichever way the subpath travels. |
| `/HighlightColor` | `[rgb array]` | unset | `pkdab`, `pkoil`: the colour highlights mix toward, instead of white. `/Highlight` still sets how far. |
| `/ShadowColor` | `[rgb array]` | unset | `pkdab`, `pkoil`: the colour shadows mix toward, instead of black. `/Shadow` still sets how far. |
| `/TipColor` | `[rgb array]` | unset | Later slice (section 7, slice L6). A colour loaded into the tip and used up first along each subpath. |
| `/TipReach` | `[fraction of path progress]` | 0.3 | With `/TipColor`: how far along each subpath the tip colour lasts. |

`/Colors` gets the new unit `array of rgb arrays`, which slice L1 adds to
the closed `FIXED` list in
`tests/capabilities.rs::paintkit_unit_is_known`. Every other unit above
is already in the list.

**Spelling.** Every existing paintkit key is American (`/ColorJitter`),
so these are too. `pkgetdef` silently ignores keys it doesn't know, so
`/Colours` would otherwise render a plain single-colour mark with no
error. Each adopting preset therefore rejects the British spellings
(`/Colours`, `/ColourMix`, `/HighlightColour`, `/ShadowColour`,
`/TipColour`) with a guard named after the preset, e.g.
`pkfan-colours-is-spelled-colors`. That is one shared helper and one
`known` test per spelling (open question Q1).

**Keys that need another key.** `/ColorMix`, `/Light` (on presets that
don't already have it) and `/TipReach` without the key they modify are
rejected (`pkfan-colormix-needs-colors`, ...), not ignored. This follows
#181's `/Stretch`-without-`/Direction` rule: an artist who wrote them
expected an effect.

### 2.2 How each preset reads the load

| Preset | `/Colors` is applied per | Head position | First pass? |
|---|---|---|---|
| `pkfan` | bristle | `pfub` | yes |
| `pktrowel` | lane, plus each `/EdgeBuildup` ridge at its own edge | `ptnc`; ridges at -1 and +1 | yes |
| `pkbroad` | striation | `pmnc` | yes |
| `pkdab` | dab, from that dab's existing outline-phase draw | `pdd` slot 5, read as a 0..1 position | yes |
| `pkoil` | only `/HighlightColor` and `/ShadowColor` in the first pass | n/a | partly |
| `pkdry` | bristle | `pbevenfrac` | later (L7) |
| `pkoil /Colors` | ridge; the base ribbon is one fill and would need splitting | `poevenfrac` | later (L7) |
| `pkliner`, `pknib`, `pkspray`, `pkribbon` | none: no head to load across | | no |

`pkdab` has no head axis (dabs are pounced at random around the spine),
so each dab takes one colour. A brush loaded with dark green and a
little yellow gives a two-tone mass. `[dark dark dark yellow]` weights
the mix toward dark, since the colours are evenly spaced. The lit side
still comes from `/Highlight` and `/HighlightColor`, which already sit on
the dabs that face `/Light`. Mapping `/Colors` onto the light axis
instead was considered and rejected: it would duplicate what
`/Highlight` does (open question Q6).

### 2.3 Which side is which

The load is **brush-relative**: the first colour sits at the left edge
looking along the travel, because that is where the paint is on a real
brush. Reverse a stroke and the colours swap sides, which is physically
right and also a trap. In alpine_lake, the left and right branches of a
pine are drawn outward from the trunk, so a brush-relative load would
light the top of the right branches and the bottom of the left ones.

`/Light` fixes that per subpath. The subpath's chord (first stop to last
stop) gives a travel direction, and if the left-hand normal, rotated by
the preset's `/Angle` where it has one, points away from the light, the
head is mirrored for that subpath. Every bristle, lane or striation of
the subpath flips together, so the mark keeps its structure. A subpath
whose chord is shorter than one `/Pitch` is not flipped: a closed
circle, a pressed fan or a single-point dab. Its first colour stays on
its left.

### 2.4 Colour arithmetic

For `N` colours at positions `p_k = k/(N-1)` and a head position mapped
to `p = (1 - u)/2` (so `u = +1`, the left edge, is `p = 0`, the first
colour):

```
k = min(floor(p*(N-1)), N-2), then t = p*(N-1) - k    (so p = 1 gives k = N-2, t = 1)
t' = ColorMix = 0 ? (t < 0.5 ? 0 : 1)
                  : clamp((t - 0.5)/ColorMix + 0.5, 0, 1)
c  = mix3(C_k, C_k+1, t')
```

`N = 1` returns `C_0`. The existing `/ColorJitter` draw is then applied
to `c` exactly as it is applied to the current colour today (`c 1 j add
shade`), so jitter keeps its meaning and its draw. `mix3(c, c, t)` is `c
+ 0*t`, which is exactly `c` in floating point, so a load whose colours
all equal the current colour paints byte for byte what the plain brush
paints. Slice L2 pins that as a test.

`/HighlightColor H` replaces `shade(c, 1 + a)` with `mix3(c, H, a)`, and
`/ShadowColor S` replaces `shade(c, 1 - a)` with `mix3(c, S, a)`, using
the same strength `a` each preset computes today. artkit's `shade` is
`mix3` toward white or black, so `H = [1 1 1]` matches today's highlight
in value. It is not guaranteed byte-identical, because the operation
order differs, but it doesn't need to be: an unset key never reaches this
branch.

### 2.5 Byte identity when unset

- Every new branch tests `opts /Key known`. With all new keys absent,
  each preset runs exactly the instructions it runs today. The only
  change on that path is one extra `known` test per key, which neither
  paints nor draws.
- No new `frnd` draws, set or unset. A loaded colour is a pure function
  of a head position the preset already computes, the subpath's chord
  and the options. `pkdab` reuses its existing slot-5 draw. Turning
  `/Colors` on therefore re-colours a mark without moving it, and a mark
  placed after the call doesn't move either.
- `pkwet` gets one bookkeeping change (section 3.4): each frame records
  the active pass's mix. That is a `put` into a frame slot that nothing
  reads unless a loaded colour is being resolved, so every existing
  `pkwet` render is unchanged.

## 3. Mechanism

### 3.1 The shared resolver (prefix `pu-`)

`pu-` has no `/name` usage anywhere under `lib/`, `examples/`,
`gallery/`, `site/`, `tests/` or `selftest/` (scanned for this doc). It
is a fresh prefix because the resolver runs inside `pkfan` and `pkoil`,
whose nested `pkribbon` calls clobber every `pk*` name.

```
opts (pkfan) puparse  ->  rec          % validates, returns a load record
rec u flip puat       ->  r g b        % colour at head position u
r g b puwet           ->  r g b        % applies pkwet's active passes (3.4)
```

- `puparse` validates `/Colors`, `/ColorMix`, `/Light` and the spelling
  guard. It is handed the caller's preset name and raises guards named
  after it, built with `cvn cvx exec` from the concatenated string
  (`pkfan-colors-must-be-an-array`). The self-tests match the resulting
  names exactly, so a concatenation bug fails them. It returns
  a record (`[colors mix lightset light]`) that the preset stores under
  **its own** prefix (`pfColorRec`, `ptColorRec`, `pmColorRec`,
  `pdColorRec`; not `pfLoad` or `ptLoad`, which already hold `/Load` and
  `/Cover`). After
  `puparse` returns, nothing depends on `pu-` globals surviving. A
  nested call can clobber them freely, which keeps the prefix rule
  intact even if `pkribbon` ever gains a load.
- `puat` and `puwet` read only their operands and the `pkwet` frames.
  They use `pu-` names only as transient scratch.
- Validation follows the file's conventions. The value must not be
  executable, checked with `xcheck` before any type check, because a
  procedure is `arraytype` too (pkpaper's `/Tone` note). It must be an
  array of 1 to 8 entries, each a 3-element array of numbers in 0..1.
  The guards are `pkfan-colors-must-be-an-array`,
  `pkfan-colors-must-have-1-to-8-colors`,
  `pkfan-colors-entries-must-be-rgb-arrays` and
  `pkfan-colors-components-must-be-0-to-1`.

### 3.2 Per preset, in PostScript terms

**pkbroad.** Today it sets the colour once per striation, before the stop
loop: `pmbase 1 pmcj add shade setrgbcolor`. With `/Colors` and no
`/Light`, the same line becomes `pmColorRec pmnc false puat puwet` followed by
`3 array astore 1 pmcj add shade setrgbcolor`. With `/Light`, a pre-pass
over `pmraw` before the striation loop fills a per-stop flip array:
subpath bounds come from the `atend` bits, the chord from the first and
last stops, and the left normal is rotated by `pmAngle`. The colour is
then set before each `pmemit` or `pmstamp` from the flip of the run's
first stop. Runs never cross a subpath, because `pmemit` is flushed at
every subpath boundary already, so one flip per run is exact.

**pktrowel.** Lanes work the same way, using `ptnc` (the lane centre,
before `/Scrape` narrowing). The two `/EdgeBuildup` ridges are set
separately: the `1 ptrwn sub 1 ptridge` ridge takes the colour at `u =
+1`, and the `-1 ptrwn 1 sub ptridge` ridge takes the colour at `u =
-1`, each darkened by the same `1 EdgeBuildup*0.5 sub shade` used today.
`ptridge` and `ptgemit` keep drawing their randomness unconditionally,
so the fixed-draw-count contract (`ptroll`) still holds.

**pkfan.** Each bristle keeps its one `pfColorJitter pkjit` draw, taken
where it is taken today, but stores it instead of applying it. The
colour is set inside `pfrun` and `pfray`, just before their `pkribbon`
call, from the bristle's `pfub`, the run's flip and the stored jitter.
That lets `/Light` flip per subpath, since a bristle's runs span every
subpath of the path. A pressed fan's rays (`pfray`) radiate through
140 degrees, so `/Colors` paints across that arc, first colour on the
left of the fan's axis.

**pkdab.** A dab's position on the load is `pdd 5 get`, the outline
phase it already draws. That couples which colour a dab gets to how its
outline is rotated, which can't be seen and costs no new draw (taking a
fresh draw would re-roll every dab after the first). The body layer uses
`puat` in place of `pdbase`. The shadow and highlight layers start from
that dab's resolved colour, and mix toward `/ShadowColor` or
`/HighlightColor` when set.

**pkoil (highlight and shadow only).** In layers 3 and 4,
`pobasecolor poHighlight 1 add shade` becomes `pobasecolor
poHighlightColor poHighlight mix3`, and the shadow layer changes the
same way.

### 3.3 Cost bounds

Resolution happens once per bristle, lane, striation or dab, or once per
run when `/Light` is set. Each resolution is two array reads, one `mix3`
and at most `depth` `mix3`s in `puwet`, where depth is at most 8 (the
`pkwet` nesting cap). The `/Light` pre-pass is O(stops), and the
existing budgets already cap stops. `/Colors` is capped at 8 entries. No
loop bound changes, so every existing safety budget still bounds total
work.

### 3.4 pkwet: the hook a loaded colour needs

`pkwet` grades a mark by setting the colour of each pass to
`mix3(base, Under, f*Pickup)` before running the procedure, and the
#181 rule is that the procedure must paint in the colour it inherits.
A preset with `/Colors` paints in its own colours, so every graded pass
comes out at full strength and a pull reads as a hard block. Prototyped
for this doc: a two-colour band (two `pkbroad` strokes, light over dark)
under `<< /Direction 270 /OneSided true /Offset 40 /Soft 1 >> pkwet`.
With absolute colours, the dark half's pull stayed solid to its full
40pt reach. With each colour mixed toward `/Under` by the pass's factor,
it graded into the water like an ordinary inherited-colour pull. That
second case was emulated by recovering the factor from the inherited
colour.

So `pkwet` publishes the factor. Frames grow from 4 slots to 7, as
follows. The extra slots go at the end, so slots 0-3 keep the meaning
the pass budget and the nesting machinery rely on.

- Slot 4 is an array of `/Layers` factors (`f*Pickup`, 0 for the core
  pass), filled in the existing plan loop. The plan loop takes no new
  draws.
- Slot 5 is `/Under`.
- Slot 6 is the index of the pass now running. It is written immediately
  before `{ pqmyframe 1 get exec } stopped`, in the same place the
  colour is set today.

`puwet` applies `mix3(c, U_d, k_d)` for `d = 1 .. pqdnow`, outermost
first, which is the order in which nested `pkwet` calls mix the
inherited colour. The core pass has `k = 0`, which is the identity, so
`/Soft 0` stays the plain call. Frames are indexed by depth and only
`1 .. pqdnow` are read, so a stale slot left by an error-unwound or
finished call is never read. Error unwinding needs no new code for the
same reason. `pkwet`'s own renders are unchanged: the writes are pure
bookkeeping and nothing reads them except `puwet`.

The rule in HANDOFF and the `pkwet` header becomes: the procedure must
paint in the colour it inherits, *or* in loaded colours, which `pkwet`
grades for you.

### 3.5 Tip load (later slice L6)

`/TipColor` is a colour used up first along a subpath: at progress `t`
the stroke's colour is `mix3(Tip, c(u), w)`, with `w = smoothstep(t /
TipReach)`. Every preset fills a run as one polygon in one colour, so
the run has to be split. To keep the random stream fixed, the split must
not resample. `pmemit` and `pfrun` call `pkjit` once per boundary point
as they emit, so the implementation first computes the run's boundary
points into an array, in the same order and with the same draws, then
fills up to 4 sub-polygons at fixed progress breakpoints, each
overlapping its neighbour by one stop (the abutting-polygon seam
gotcha). Where the preset has `/Depletion` (pkbroad), `/TipReach`
defaults to `Depletion`-scaled reach rather than a constant (open
question Q5). This slice is deliberately last: it is the only part of
the design that changes how a run is filled.

## 4. Interactions

### 4.1 With CANVAS_SURFACE.md (#182)

- **Keys are disjoint.** This doc adds `/Colors`, `/ColorMix`,
  `/HighlightColor`, `/ShadowColor`, `/TipColor` and `/TipReach`, and
  extends `/Light` to three more presets with `pkdab`'s meaning. #182
  adds `pkcanvas` with `/Seed`, `/Weave`, `/Mottle` and `/Relief`, and
  the preset key `/Touch`. No key is shared and no existing meaning
  changes.
- **Prefixes are disjoint.** `pu-` here, `pj-` there.
- **Spaces differ on purpose.** `/Light` is in user space, like
  `pkdab`'s, so a reflected tree under `1 -1 scale` gets a mirrored lit
  side, which is what a reflection shows. The canvas surface is in
  canvas space, so the same reflection catches on the same canvas weave,
  unmirrored. Both are what a painter sees.
- **They compose.** A touched, double-loaded knife resolves its colours
  per lane and is then clipped to the surface's catch. Neither changes
  the other's draws or geometry.
- **Shared presets.** Both designs edit `pktrowel`, `pkbroad` and
  `pkfan`. The slice plans order those edits one preset at a time (see
  CANVAS_SURFACE.md section 7 for the combined order) so that two PRs
  never rewrite the same preset body concurrently.
- **Unit list.** Only this design adds a `FIXED` unit (`array of rgb
  arrays`). #182's keys use existing units.

### 4.2 With directional pkwet (#181)

Covered in 3.4. Without the hook, a double-loaded pine or knife stroke
could not go into the directional reflection pull that #181's NOTES name
as alpine_lake's follow-up. With it, the pull grades as it does for an
inherited colour. `/Light` and `/Direction` are both user-space angles
and don't interact.

### 4.3 With the brush motions (#184)

The motions build paths and never touch colour, so any motion feeds a
loaded brush. Which side the first colour lands on follows each motion's
travel (read from the generators):

| Motion | Travel of each subpath | Brush-relative load gives | With `/Light` |
|---|---|---|---|
| `pkcrisscross` | left to right (cos of +-Angle is positive) | first colour on the upper side of every X stroke | first colour faces the light |
| `pkpull` | along `/Direction` (-90 pulls down) | a pull down puts the first colour east, a push up puts it west | the same side for both |
| `pktap` | along `/Direction` (90 stabs up) | first colour west of each stab | faces the light |
| `pkzigzag` | one subpath along the spine, legs alternating | first colour above the spine on both legs | chord = the spine, so one flip for the whole zig-zag |
| `pkscumble` | one subpath of loops, travel turning through 360 degrees | the colours swirl with the loop, which is how a scumbled double load really looks | the chord is the whole chain, so one flip; the swirl stays |
| `pkscumblein` | closed circles | first colour on the left of the circle's start | chord under one `/Pitch`, so no flip |

## 5. Ghostscript

Everything here is plain PostScript arithmetic: `mix3`, array reads,
`known`, `xcheck`. Nothing new depends on `rand`, `setalpha` or anything
gs lacks, so the presets stay as portable as they are today. Under gs
only the existing seeded draws differ (gs's `rand` is another
generator), and colour resolution is deterministic given those draws.
The gs driver runs with `true setpacking`, as `ghostscript_accepts_
paintkit` does. `/Colors` is a literal array, which packing doesn't
pack, but the driver's `/Pressure` procedures still exercise the packed
branch.

## 6. Test plan

**Self-tests** (`%%SelfTest` next to each guard, `pscat --selftest`):
- `pkX-colors-rejects-malformed-loads`, one block per adopting preset:
  each `mustguard` above (not an array, executable, 0 or 9 entries, a
  2-element entry, a component of 1.5), plus `mustpass` for 1, 2 and 8
  colours.
- `pkX-colors-spelling`: `/Colours` and `/HighlightColour` raise
  `pkX-colours-is-spelled-colors`, and `mustpass` for `/Colors`.
- `pkX-colormix-needs-colors` and `pkX-light-needs-colors` (pkfan,
  pktrowel, pkbroad), plus `/ColorMix 1.5` out of range.
- `puat` as a pure function, with `mustbe`: `p = 0` returns the first
  colour, `p = 1` the last, `/ColorMix 0` steps at the midpoint, and
  `N = 1` returns the colour.
- `puwet` outside any `pkwet` is the identity (`mustbe`). Inside `<<
  /Soft 0 >> pkwet` it is also the identity.

**Rust tests** (`tests/paintkit.rs`, new section "loaded colours
(issue #183)"):
1. `loading_unset_is_byte_identical`: each first-pass preset with the
   new keys absent against a pinned pixel hash taken from `origin/main`.
   This backs up the cross-file check below.
2. `a_uniform_load_is_the_plain_brush`: `/Colors [c c]` with the current
   colour `c` renders pixel-equal to the plain call, for pkfan, pktrowel
   and pkbroad (exact, per 2.4).
3. `loading_takes_no_random_draws`: a mark drawn from the caller's
   stream after the preset doesn't move between `/Colors` unset and set
   (the `trowel_consumes_a_fixed_number_of_random_draws` pattern).
4. `double_load_puts_the_first_color_on_the_left`: a left-to-right
   two-colour `pkbroad` at `/ColorMix 0` has its upper-half pixels
   nearest `C0` and its lower-half pixels nearest `C1`. A right-to-left
   stroke swaps them, and `/Light 90` puts `C0` on top for both.
5. `colormix_grades_between_bands`: a column profile across the band
   has one step at `/ColorMix 0` and a monotone ramp at 1.
6. `edge_buildup_takes_its_edges_color`: pktrowel's ridges are a
   darkened `C0` on one edge and a darkened `C1` on the other.
7. `highlight_color_moves_the_hue`: pkdab and pkoil highlight pixels
   move toward `/HighlightColor`'s hue, not toward white. The same test
   covers `/ShadowColor`.
8. `pkdab_colors_mix_per_dab`: with `[A B]` at `/ColorMix 0`, body
   pixels cluster near A or B, roughly half each over a 400-dab clump.
9. `wet_pull_grades_loaded_colors`: a two-colour `pkbroad` inside a
   one-sided directional `pkwet`. The outermost pass's pixels sit near
   `/Under`, not near `C1`. The test fails if `puwet` is removed, which
   is checked by mutation.
10. `wet_frame_slots_do_not_change_pkwet`: `examples/paintkit_wet_demo.ps`
    and `paintkit_wet_pull_demo.ps` match their pinned hashes.
11. `every_documented_parameter_changes_the_render` (the pkbroad
    pattern): extend it to the new keys on each adopting preset.
12. `ghostscript_accepts_paint_loading`: a driver using every new key on
    every adopting preset, inside a directional `pkwet`, under `true
    setpacking`, checking that gs exits 0.
13. `tests/capabilities.rs`: `array of rgb arrays` added to `FIXED`, so
    `every_paintkit_parameter_states_its_unit` passes. The existing
    `/Load`-is-`[per /Width]` pin is untouched.

**Byte identity across the repo** (#141's method, run in every slice
that edits a preset or `pkwet`) is `scripts/identity_check.sh` (#263):

```sh
git worktree add --detach ../base-wt-<N> "$(git merge-base HEAD origin/main)"
scripts/identity_check.sh ../base-wt-<N> --gallery-still --control path/to/control.ps
git worktree remove --force ../base-wt-<N>
```

- It renders every `examples/`, `gallery/` and `selftest/drivers/` file
  that loads paintkit (21 files, 30 page renders at #141) from both
  trees with the same release binary, as PNG only, and compares the
  bytes. Every page must be `identical`. The base worktree must be at the
  branch's base commit (after the step 5.5 rebase), not a newer
  `origin/main`.
- `--gallery-still` also compares `gallery/alpine_lake.ps` at 2x with the
  committed `gallery/renders/alpine_lake.png` byte for byte, in each tree.
- `--control` is the negative control. For this design it is a driver
  that uses a key only the branch has (a `pkbroad` with `/Colors`): it
  must render *differently* in the two trees, or the script exits 2,
  because otherwise both trees may have loaded the same library.
- Record the file and render counts in the PR, since the count grows as
  examples are added.

## 7. Slice plan

Each slice is one small PR, gated by the full CI (fmt, clippy, cargo
test, `selftest.sh`) and the byte-identity check above.

| Slice | What | Model | Difficulty |
|---|---|---|---|
| L1 | `pu-` resolver (`puparse`, `puat`, `puwet` as the identity until L3), spelling guard, the `array of rgb arrays` unit, self-tests for the resolver. No preset changes. Every helper carries `% @internal`, or `every_migrated_file_names_match_the_catalog_exactly` fails, and any load-time definition that isn't `/name ... def` is written value-first to fit build.rs's two-slot window. | sonnet | medium |
| L2 | `pkbroad` and `pktrowel` adopt `/Colors`, `/ColorMix` and `/Light`, including the per-subpath flip pre-pass and the edge-buildup colours. Rust tests 1-6 for both. | sonnet | medium |
| L3 | `pkwet` publishes its pass mix (frame slots 4-6), `puwet` applies it, tests 9-10, HANDOFF gotcha reworded. Kept separate because `pkwet`'s frame machinery has been the most-reviewed code in the file. | opus | medium |
| L4 | `pkfan` adopts the load, with the jitter draw stored per bristle and the colour applied per run (pfrun/pfray). | sonnet | medium |
| L5 | `pkdab` gets `/Colors` per dab, plus `/HighlightColor` and `/ShadowColor` on `pkdab` and `pkoil`. Tests 7-8. | sonnet | easy |
| L5b | Specimen `examples/paintkit_loading_demo.ps`: a double-loaded fan-brush evergreen (with `/Light`), a white-to-blue knife snow pull, a two-colour broad stroke, and one of the three inside a directional pull. Catalog check, psart skill, README, NOTES. Test 12. | sonnet | medium |
| L6 | Optional: `/TipColor` and `/TipReach` (3.5) on pkbroad, pktrowel and pkfan. | opus | hard |
| L7 | Optional: `/Colors` on `pkdry`, and on `pkoil`'s ridges and split base. | sonnet | medium |

L1 lands before L2-L5. L3 lands before L5b, because the specimen's pull
needs it. Converting `gallery/alpine_lake.ps` to loaded brushes changes
a committed render, so it belongs to #187 (the second specimen) or its
own issue, not here.

## 8. Open questions for the maintainer

Each is phrased as a decision, with a recommendation.

- **Q1. Spell it `/Colors` or `/Colours`?** Recommend `/Colors`, matching
  `/ColorJitter` and `setrgbcolor`, with a guard that rejects the
  British spellings rather than silently ignoring them. The alternative
  is to accept both as aliases. That doubles every key in the family
  with no gain, and #141's alias rule is for renames.
- **Q2. Brush-relative by default, with `/Light` as the world-space
  override?** Recommend yes. A load is paint on the brush, and a
  world-space default would make a pull and a push identical, which no
  brush does. `/Light` reuses `pkdab`'s exact meaning instead of a new
  key such as `/ColorFace`.
- **Q3. Default `/ColorMix`?** Recommend 0.3: a double-loaded brush with
  a short blended seam, which is the look the issue describes. 1 (a full
  gradient) is one key away.
- **Q4. Include the `pkwet` hook in the first pass?** Recommend yes, as
  its own slice (L3) before the specimen. Without it, `/Colors` marks
  silently break the directional pull that alpine_lake's next revision
  needs. Without the hook, the HANDOFF rule would also have to forbid
  wrapping a loaded brush.
- **Q5. Build tip loading at all, and should `/TipReach` follow
  `/Depletion`?** Recommend deferring it (L6) until the three specimens
  have been looked at. It is the only part that changes how runs are
  filled, and side-to-side loading covers all three acceptance rows.
  If it is built, tie the default reach to `/Depletion` where the preset
  has one.
- **Q6. `pkdab /Colors`: per-dab mix or along the light axis?**
  Recommend per-dab mix (2.2). The light axis is already covered by
  `/Highlight` plus `/HighlightColor`.
- **Q7. Allow a 1-colour `/Colors`, and cap at 8?** Recommend allowing 1
  to 8. One colour is an absolute colour that `pkwet` still grades,
  which is useful when the colour is computed. Eight is more than any
  brush carries and keeps resolution O(1).
- **Q8. Does `/Colors` fully replace the current colour?** Recommend
  yes, with no "current colour" placeholder entry. With the L3 hook, the
  current colour has nothing left to do for a loaded brush, and a
  placeholder is a second way to say the same thing.
