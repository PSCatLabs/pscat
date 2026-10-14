## Typewriter: reference study and design, awaiting approval (issue #210, 2026-10-14)

#210 is the first ticket of epic #209. It delivers the reference board
(`docs/typewriter/REFERENCES.md`, 15 crops from public-domain scans) and the
design (`docs/TYPEWRITER.md`). No library code was written. The maintainer
approves the design before #211 starts.

### What was done

- **Sources.** Seven public-domain scans: US federal records via Commons/NARA,
  plus one JFK Assassination Records release PDF, dated 1914 to 1961. Also NBS
  Circular C431 (about 1941) on ribbons. Pitch and dpi were cross-checked from
  the ratio of line pitch to character pitch (a 1/6-inch line), so the pica/elite
  verdicts don't rely on the files' dpi tags.
- **Measurements.** Tesseract boxes plus per-line robust fits
  (`docs/typewriter/tools/measure.py`). Real per-strike displacement is
  0.15–0.3pt sd. Per-letter machine offsets are about 0.07–0.11pt, after the
  face's own shape is removed by averaging across documents. Line tilt is about
  0.1°. Ink coverage varies 7–21% per strike. The conclusion: the look is mostly
  ink, not jitter.
- **Prototype.** `docs/typewriter/prototypes/tw_proto.ps` is a Type 3 font
  drawing embedded outlines with keyed-hash jitter, a weave clip, an ink-spread
  stroke and filled counters. It runs in pscat and gs. A 2,500-cell page at
  300 dpi takes 0.62s (pscat) and 0.28s (gs) of CPU.

### Decisions (pending approval)

- Base face: Courier Prime outlines (OFL), embedded as generated path data, so
  pscat and gs draw the same face. Liberation Mono (pscat's `/Courier`) leans
  sans and is not gs's `/Courier`. Special Elite bakes its wear into each glyph.
- Pica and elite. `/Size` is separate from `/Pitch`, because elite caps
  measure about 7.7pt, larger than a Courier scaled to a 6pt advance.
- Variation in three layers (machine, line, strike), from integer-keyed hashes,
  with a strike index for overtypes and one `/Wear` knob.
- Ink: a `rectclip` weave (holes are a clip, not paint), body fill, spread
  stroke, counter fill, and a one-side-heavy overlay. Uses gs's guarded
  `.setfilladjust2`.
- Budget: 5s of CPU per page in each interpreter, advisory only. CI renders
  strips under the existing 2s cap.

### Findings worth keeping

- gs replays a Type 3 glyph built under `setcachedevice`. A probe of `eee` with
  position-hashed offsets came out identical in gs, and different under
  `setcharwidth`.
- pscat's `def` rejects integer keys, where gs accepts them (#338). The
  prototype uses `put`.
- `ln` is an operator: a variable named `ln` inside a `bind`-ed procedure
  becomes a call to the natural log.

### Not sourced (open questions Q3)

Two-colour ribbon bleed, punctuation embossing, and `xxxx` x-outs. None was
found in a bounded search: Commons, the web, and an OCR scan of 48 JFK release
PDFs.
