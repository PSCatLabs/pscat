# Taste: what a render must get right

The owner's taste, written down so an agent can judge a render without asking (#674). The
perceptual reviewer in `SDLC.md`, "Who merges", tier (b), checks before/after images against this
page and the issue's perceptual acceptance check. Keep it short and concrete: a rule goes in only
when the owner has stated it or rejected a render for breaking it. This file is a CODEOWNERS path,
so only the owner changes the bar.

## The purpose comes first

- **The tools must let an AI agent make art.** Different agents have different tastes, and that is
  fine. A render that works ships. A nitpick ("I would have made the sky bluer") is not a fail: it
  goes to the parent epic, which is parked by default.
- **Each area has a done-definition: the tools are available and work as documented.** Not endless
  polish. When an area's tools work, stop refining it and move on.
- **The perceptual check beats the metric.** An issue's metric (a contrast ratio, a timing, a byte
  count) never wins over its perceptual acceptance check ("reads easily at gallery size"). A render
  that hits the number and fails the look fails.

## Handwriting and board lettering

- **Sloppy but legible.** Handwriting, chalk and the strokeglyph hands exist to look human-written.
- **Fix a glyph only if it misreads as another character** (a 4 that reads as "ϟ", a 7 that reads
  as ")"). Use the smallest change that removes the misreading, and keep the curl, wobble and
  asymmetry.
- **Never tidy it.** A more regular, more geometric glyph that "reads slightly better" but looks
  less human is a regression (PR #441). Among variants that read correctly, pick the most human.

## Typed and printed text

- **Typed-letter darkness is fairly even and readable.** Variation is welcome (ribbon, strike force,
  wear), but never very dark next to very light. That was rejected twice (#591, #594).
- **Copy erosion must not fade whole words.** Dropout, wear and generation loss may take bits of
  letters. A reader must still be able to read every word.

## How to judge

- Judge at the size the output is used: a gallery still at gallery size, a specimen at its page
  size. Zoom in only to find the cause of something visible at that size.
- Compare against the issue's intent, not against what you would have made.
- A fail names the image, the region and the rule above (or the acceptance check) it breaks.
