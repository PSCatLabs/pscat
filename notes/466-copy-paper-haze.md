# #466: a photocopy's paper keeps a grey haze and a grain

Follows #214 and #215 (read `notes/214-released-look.md` first). In #215's blind
comparison the flat white of a `/Generation` copy was the easiest tell against
the real scans (T2, T9 and T14 are grey and mottled).

## What shipped

- `rel-page /Haze` [0..1], default 0.6, in `lib/released.ps`. It applies only
  when `/Generation` is above 0, so an original is untouched. `/Haze 0` is the
  old white copy, byte for byte.
- The haze is the copier curve's output times `1 - 0.16 Haze`, done in
  `rlSet`/`rlCol` after `rlMono`, for everything drawn under the copy curve:
  paper cells, folds, punch-hole rings, the other marks. Dark things (type,
  stamp ink) barely change. The haze has to come after the curve: at
  `/Generation 1` the curve sends any darkness under about 0.175 to pure white,
  so a grey painted before it comes out white.
- The paper field of the copied original is laid in 3pt cells (it was 9pt) with
  a per-cell haze: nine Gaussian blotches plus two octaves of value noise
  (14pt and 6pt lattices). A fine grain of tiny lighter and darker flecks goes
  on after the paper, under the type, set directly (not through the curve).
  Foxing grains and fibres are skipped on a hazed copy (they showed as pale dots).
- The last sheet's base field and the edge shadow's inner colour carry the same
  haze, so the lid fades into the grey, not white.
- Everything uses `rlU` under `rlSeed 16` (the blobs) and `17` (the flecks), so
  pscat and gs draw the same page and the existing seeded streams did not move.

## Calibration (300 dpi, bright blocks without ink, 6px blocks and 48px blocks)

| | mean | grain, 6px | mottle, 48px |
|---|---|---|---|
| T14 (a copy) | 0.907 | 0.009 | 0.011 |
| T2 | 0.844 | 0.012 | 0.006 |
| T9 | 0.856 | 0.009 | 0.005 |
| copy sheet, plain (`/Generation 1`, no lamp, edge or folds) | 0.902 | 0.0028 | 0.0105 |
| specimen gen 1 (with its `/Lamp 0.25 /EdgeDark 0.5 /Folds 0.5`) | 0.901 | 0.0035 | 0.024 |

On a plain copy the tone and the mottle match T14 (0.902 against 0.907, 0.0105
against 0.011); the 48px figure of the specimen is larger only because its lamp,
edge darkening and folds are on (T2 and T9 are scans of single sheets, so they
are less mottled). The fine grain is about a third of the real scans' at every
setting: denser grain costs hundreds of thousands of fills (follow-up #474, done: `notes/474-copy-paper-haze-grain.md`).
T15 is bitonal and white, so the specimen's third sheet sets `/Haze 0.2`.

## Renders that changed

`scripts/identity_check.sh --loader lib/released.ps` (control: a driver that
draws a hazed copy only if `/Haze` is a known key, which differs): exactly the
two files that use a generation differ, `examples/released_specimen.ps` and
`gallery/released_memo.ps`. With the default set to 0 temporarily (and the base's
specimen) both are identical, so the change is gated by the key alone.
Regenerated: `docs/dev/design/released_specimen.png` and `_gs.png`,
`gallery/renders/released_memo.png`, and the boards that are made from them
(`release_vs_real.png` with a new before/after panel against T14,
`blind_grid.png`, `memo_vs_real.png`: the blind grid's order is the same fixed
shuffle, and its key stays in the PR, not here).

## Not done

- Pixel-level grain at the real scans' strength, and the crumpled-paper wave of
  T14's mottle (smooth blobs here): #474.
