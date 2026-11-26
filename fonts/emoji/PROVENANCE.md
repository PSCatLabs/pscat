# Twemoji Mozilla: provenance and licence

`TwemojiMozilla.ttf` is the colour emoji face pscat compiles in (issue #1425).
`show`, `stringwidth` and `charpath` reach it from a Unicode-mode font when no
text face has the character (`docs/dev/FONTS.md`, "Unicode text, fallback faces
and colour emoji"). It is never a `findfont` name.

## The file

| File | SHA-256 | Size | Version |
|---|---|---|---|
| `TwemojiMozilla.ttf` | `6d90152ee0d29e82fe2a87793af5aa4b7ad13e6538360889e141e81ed299ee8e` | 1,474,284 bytes | twemoji-colr 0.7.0 (Twemoji 14.0.1, Emoji 14.0) |

Downloaded unchanged on 2026-11-26 from the release asset
<https://github.com/mozilla/twemoji-colr/releases/download/v0.7.0/Twemoji.Mozilla.ttf>
(tag `v0.7.0`, commit `c0460fdb2654dee584e75c4913bfa9fd2f873338`, published
2022-10-28), renamed only (the space-free name suits `include_bytes!`). The
release's own commit log names the art: "Update to Twemoji 14.0.1 (Emoji
14.0)", followed by "Bump version to 0.7.0".

It is a COLR version 0 / CPAL font: each emoji is a stack of solid-colour
outline layers (3,689 colour glyphs, one palette, at most 44 layers per glyph,
512 units per em), plus one GSUB `ccmp` ligature lookup that joins flag,
keycap, skin-tone and ZWJ sequences. pscat paints the layers itself; nothing in
the file is executed.

## Licence

- **The emoji art** is Twemoji, copyright Twitter, Inc and other
  contributors, licensed under **CC BY 4.0**
  (<https://creativecommons.org/licenses/by/4.0/>). Source:
  <https://github.com/twitter/twemoji> (the graphics were later continued at
  <https://github.com/jdecked/twemoji>).
- **The font build** (the twemoji-colr tooling that turned the SVGs into this
  font) is copyright 2016-2018 Mozilla Foundation, licensed under
  **Apache-2.0**.

`LICENSE.md` here is the twemoji-colr repository's licence file at tag
`v0.7.0`, verbatim. It states both terms.

Attribution, as CC BY 4.0 asks: "Emoji graphics by Twemoji (Twitter, Inc and
other contributors), CC BY 4.0, via Mozilla's twemoji-colr 0.7.0." No change
was made to the graphics or the font file.
