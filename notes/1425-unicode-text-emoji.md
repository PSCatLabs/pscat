## UTF-8 text, fallback faces and colour emoji in show, stringwidth and charpath (issue #1425, 2026-11-26)

psanim (PSCatLabs/psanim#45, #54) needs a text-message conversation with
emoji, credits with accented names and signage. Until now a shown string was
bytes through a 256-slot Encoding, except on four Unicode-mode catalog faces,
and a character a face lacked advanced blank without a word.

**What was built.**

- `/PSCatUnicode true` in any outline font dict makes it Unicode-mode: `show`,
  `stringwidth` and `charpath` read UTF-8 and look each character up by code
  point (`font::UNICODE_KEY`, `font::outline_unicode`). Byte-mode fonts are
  untouched, so Ghostscript parity holds there (gs ignores the key).
- A fallback chain over the compiled-in faces for a character the face lacks:
  Liberation Sans, Serif, Mono (by `cmap`), then URW Symbol and ZapfDingbats
  (by glyph name through their encodings: their `cmap`s hold byte codes only).
- Colour emoji from a bundled, openly licensed vector set: Twemoji 14.0.1 in
  Mozilla's COLR v0 font (`fonts/emoji/`, CC BY 4.0, provenance and SHA-256 in
  `fonts/emoji/PROVENANCE.md`). Layers are painted through `Gfx::fill_path_rgb`,
  so emoji are vector in PNG, SVG and PDF. GSUB ligatures join flags, keycaps,
  skin tones and ZWJ sequences. `Emoji_Presentation` (Unicode 14) and
  U+FE0F/U+FE0E pick emoji or text.
- A character nothing has draws the font's `.notdef` box and is reported once
  (stderr and `Interp::font_warnings`, so npm `warnings` and pscat-mcp see it
  too), at most 32 per interpreter. Korean, Japanese and Thai name the catalog
  face that has them.
- `stringwidth` and `charpath` run the same resolver as `show`, so widths
  agree (tested on a mixed string).

**Design calls.**

- *A dict key, not new font names or Type 0.* Composite fonts are #1272 and
  follow the PLRM and Ghostscript; this is a pscat extension in the style of
  `/UnicodeBuildChar`, and the key survives `scalefont` and the copy idiom.
- *Twemoji COLR v0 over Noto Color Emoji.* Noto's vector build is COLR v1
  (gradients), which would need gradient fills in all three outputs; OpenMoji
  is CC BY-SA. Twemoji v0 is solid-colour layers, which the existing fill path
  draws as is. Its cost is the CC BY 4.0 term: the package licence expression
  gains `CC-BY-4.0`, and THIRD_PARTY_NOTICES.md carries the credit (an owner
  decision on the PR).
- *No catalog faces mid-show.* Loading NotoSansKR/JP/Thai (10 MB each) during a
  `show` raises memory and time charging, a stale negative cache and a
  directory scan per glyph. The compiled-in chain covers psanim's needs
  (Latin, Greek, Cyrillic, emoji); the warning names the face to use for the
  rest. Follow-up.
- *One glyph step per scalar.* The scalars an emoji ligature takes in stay
  steps of their own that draw nothing at zero width, so `glyph_count`,
  `kshow`, `cshow` and `xshow` keep one step per code point.
- *Controls and ignorables.* A `\n` left in a string is blank with byte mode's
  missing-glyph advance and unreported; joiners and variation selectors are
  zero width (advisor, plan review).
- *Only the bundled face is COLR-painted.* A user COLR font keeps monochrome
  outlines, so no hostile COLR data reaches the painter.

**Limits charged.** Layer fills go through `fill_path_direct` with the show's
probe and work; the ligature search (U+1F468 starts 311 ligatures, up to 9
components each) and the symbolic-name scans add to `work`; registering an
embedded fallback face is held against the memory cap. Nothing is read from
disk. `tests/unicode_text.rs` checks the step count rises for both.

**Size.** See the table below (release build, aarch64-apple-darwin; raw
`cargo rustc --release --target wasm32-unknown-unknown --crate-type cdylib`,
before wasm-bindgen/wasm-opt).

| Artefact | Before (feature/0.8.0 @ 90a130d7) | After | Change |
|---|---|---|---|
| `pscat` release binary | 16,883,936 | 18,413,184 | +1,529,248 |
| raw wasm (`pscat.wasm`) | 13,082,473 | 14,609,115 | +1,526,642 |
| `.crate` (`cargo package`) | 5,877,722 | 6,627,968 | +750,246 |
| npm tarball | 5,952,081 | 6,716,026 | +763,945 |
| npm unpacked | 13,667,989 | 15,218,105 | +1,550,116 |

The npm caps in `scripts/build_npm.sh` went from 6 MiB / 13.5 MiB to 7 MiB /
15.5 MiB. #770 (zstd-compressing the embedded fonts) would take back about
half of the binary and wasm cost.

**Render identity.** The two-binary identity check over every tracked
`examples/`, `gallery/` and `selftest/drivers/` file (171 files, 210 pages)
is byte-identical, with a control that differs: no existing render changed.

**Deferred.** Shaping, GPOS and bidi (Arabic and Hebrew come out in logical
order); COLR v1; style-matched fallback (the chain is regular weight); Latin-1
recovery of invalid UTF-8 (U+FFFD, reported); loading the catalog's CJK and
Thai faces mid-show; an `emoji` cargo feature for builds that want the 1.4 MB
back; the #1412 lint for UTF-8 in a byte-mode font.
