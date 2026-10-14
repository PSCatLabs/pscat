# #349: four gallery stills "no longer match" (2026-10-14)

Premise of the issue was half wrong. #344's harness rendered gallery pieces at
`--dpi 144` on the `%%BoundingBox` page, which is not how the stills were made.
Re-rendering each with its own recipe (`gallery/README.md` "Re-rendering the
stills": `2 2 scale` prepended, `--page` doubled, page sizes from `show.sh`'s
`PAGES`; `hortus` is a 1x 620x800 still) gives:

| still | pixels differing | max channel delta | verdict |
|---|---|---|---|
| `hortus` | 0 (byte-identical) | 0 | never stale; harness mismatch only |
| `keepin_it_real` | 0 (byte-identical) | 0 | never stale; harness mismatch only |
| `hundred_lines` | 25 of 1,388,800 | 25 | harmless, re-rendered |
| `varsity_backs` | 101 of 2,772,000 | 62 (3 px over 32) | harmless, re-rendered |

Cause of the two real diffs: `git bisect run` (byte-equality with the committed
still as the test) lands on 4d26e38, "Fix hollow centre when a tiny curve is
stroked wider than its diameter" (#170). It flattens tiny curves before
stroking, so the full-stop dots in `hundred_lines` and a few glyph-edge pixels
in `varsity_backs` anti-alias slightly differently. Both stills were
byte-identical on the commit before it (`4d26e38~1`). #289, #317, #327, #300,
#332 and #344 are not involved.

Visual check: `349-stale-stills-zoom.png` (old | new | diff x8, zoomed). No
shape, colour or layout change anywhere; the diff is single pixels on stroke
edges and the dots.

Not done: `hortus` and `keepin_it_real` were left untouched. `identity_check.sh`
compares only `alpine_lake` against a still, so nothing in CI sees these four.
