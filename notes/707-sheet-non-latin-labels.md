## `pscat sheet` labels in the script they are written in (issue #707, 2026-10-23)

`pscat sheet` set every label in Helvetica through the standard Latin encoding, so `café`,
`한글`, `日本語` and `ไทย` came out as `caf?`, `??`, `???` and `???`. Now each character
picks its face by script, inside `contact_sheet::render_text`.

**No existing picker to reuse.** Nothing in pscat chose a face by script before: the Stage
22/23 faces are picked by name in the program (`/NotoSansKR-Regular findfont`), and
`examples/international_text.ps` does that by hand. What is reused is the Stage 22
Unicode-mode catalog faces (`show` decodes UTF-8 and maps through `cmap`), and the
missing-pack path (`findfont`'s `invalidfont` plus the "install the pack" hint). The
classifier is private to `contact_sheet.rs` (codepoint ranges to `Face`).

**Faces.** ASCII and Latin-1 letters: Helvetica / Helvetica-Bold, as before. Latin-1
accents need a byte encoding: the label's Latin run is redefined under
`ISOLatin1Encoding` and its accented bytes are sent as octal escapes (only when the run has
one; pure ASCII uses the unmodified face). Hangul: Noto Sans KR. Kana, Han and CJK
punctuation: Noto Sans JP. Thai: Noto Sans Thai. Space and ASCII punctuation join the run
before them, so `日本語 (logo)` does not flip faces at the space.

**Caveats, all deliberate.**
- Noto Sans has one weight here, so a CJK/Thai label is regular while a Latin one is bold.
- No Simplified-Chinese face ships. Han is set in the Japanese face, whose cmap lacks some
  Simplified-only characters (`简`, `们`, `这`). Those draw as `?`, never as a blank gap
  (`font::has_glyph` checks the cmap). A Simplified-Chinese face would fix it.
- Thai is unshaped (FONTS.md): stacked vowel and tone marks use their own advances, so
  `ฟื้น` looks loose. Legible, not typeset.
- Latin Extended (`ł`, `č`), Cyrillic, Greek, Arabic and emoji are still `?`.
- The three Noto faces live in the `cjk-thai` pack. Without it the characters draw as `?`
  and the sheet prints one warning per face, with pscat's usual hint to run
  `pscat fonts install`. The sheet still renders and exits 0.

**Strip height.** A label with a Noto run gets a 1.4 em strip (baseline 0.36 em up) instead
of 1.3/0.3: measured ink reaches 0.84 em above and 0.26 em below the baseline (Thai
below-vowels), which left under a pixel at 16 px. A Latin-only strip is exactly as before.

**Check.** A Latin-only label is byte-identical to the pre-#707 renderer: a unit test holds
the old pipeline as a reference, and `pscat sheet` from the base binary and the head binary
were compared PNG to PNG over six sheets (`--label` metacharacters, `--no-labels`,
`--no-sizes`, zoomed cells, default stem labels); a negative control with `café 日本語`
differs.
