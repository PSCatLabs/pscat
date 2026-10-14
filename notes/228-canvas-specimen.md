## Canvas surface S4: the specimen, docs and test 6 (issue #228, 2026-10-13)

Slice S4 of #182's design (`docs/CANVAS_SURFACE.md` 6 and 7). All five `/Touch`
adopters are merged (S2 pktrowel and pkdry, S3 pkfan, pkbroad and pkliner), so
this slice adds no rendering code: `lib/` is untouched, and byte identity holds
by construction (`scripts/identity_check.sh` needs a `--control` that uses a key
only the branch has, and S4 adds none).

- **`examples/paintkit_canvas_demo.ps`** (780x640, committed render at
  `docs/paintkit_canvas_demo.png`, 2x supersampled like the gallery stills).
  `pkcanvas` is called once, first, in page space, before any panel translates.
  Top row: the same dry brush and palette knife crossing, three times with the
  same seeds: full contact, each tool's own break-up (`/Load`, `/Coverage`), and
  the shared canvas (`/Touch 0.7` brush first, `/Touch 0.55` knife across it).
  Bottom row: a `/Touch` sweep (0.15 to 1, with 1 labelled "= unset"), a
  three-colour knife and fan under `/Colors` and `/Touch 0.65`, and a touched
  knife stroke pulled down inside a directional `pkwet` beside an untouched one.
- **Why those touches.** At equal `/Touch` a second opaque stroke covers the
  first on exactly the same knuckles, so the crossing hides the sharing. The
  heavier stroke goes first with the higher touch, so the knife (lighter) lands
  on a subset of the brush's knuckles and the crossing reads as one canvas.
  First attempt used 0.6 and 0.45, which was too sparse to read at page scale;
  0.7 and 0.55 keep the patches of catch and miss without losing the strokes.
  Looked at a 2x crop of the crossing to confirm the weave is shared: the hole in
  the middle of the crossing is in both strokes.
- **Dark ground.** Light strokes on a dark ground, so the knuckles read.
- **Test 6** `the_weave_is_in_canvas_points_at_any_dpi`: a touched `pkbroad` band
  at 72 and 144 dpi, compared in 10pt blocks. Mean block difference 0.24 against
  `tests/golden.rs`'s tolerance of 6. The weave is coarse (8pt) so the pattern
  lives at block scale (at the default 2.5pt every seed averages alike), and two
  negative controls fall far outside: a weave sized in pixels (61.7) and another
  seed (38.0).
- **`ghostscript_accepts_the_canvas_demo`**: gs runs the specimen (no alpha).
- **Docs.** README paragraph with an example and the render; psart skill bullet
  (touch ordering for crossing strokes); the HANDOFF gotcha "a touched mark is
  clipped in canvas space; set the canvas first".
- **Catalog check.** `pkcanvas`'s `@summary` already names all five adopters
  (S3), so no edit to `lib/paintkit.ps` was needed (L5, #226, is editing it).
- Not here: S5, which is #231 (more adopters, a `/Box` wrapper, a `/Touch`
  procedure, a ground tone). No site card: the render is committed under `docs/`.
