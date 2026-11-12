# #1051: accented Latin letters in strokeglyphs

`sgpath` had no accented Latin letters, so a cafe sign's `café` raised
`sg-missing-glyph:é`, and `(caf\351)` failed the same way. No existing glyph,
advance or committed render changed (see Evidence).

## What was added (lib/strokeglyphs.ps)

- **Every letter of Latin-1 and Latin Extended-A.** That is U+00C0-00FF
  (except × and ÷, which were already there as operators) and U+0100-017F,
  plus Romanian ș ț (U+0218-021B). There are 161 composed letters, 17 barred
  or apostrophe forms, 6 run-together pairs, 9 authored letters, and ı and ȷ.
- **Composed, not authored.** `sgaccents` holds 15 accent strokes (acute,
  grave, circumflex, caron, breve, tilde, macron, dieresis, dot, ring,
  double acute, turned comma, cedilla, comma below, ogonek), each with an
  anchor: `/top`, `/bot` or `/ogonek`. `sgcomposed` is generated from
  `unicodedata.decomposition` (base + combining mark), with a dx dy nudge
  for bases whose box centre is not the body's (a's tail, J/j stem, L/l
  stem, r's shoulder, C and t cedilla, e and U ogonek). `sgaccented` (run
  by `sginit`) builds each one as the base's own skeleton plus the accent.
  So the accents take the base's hand and jitter, and a reshaped base
  reshapes its accented forms.
- **Placement.**
  - `/top` accents sit 90 units over the box (x-height letters: foot at 590).
  - Over a capital or an ascender (box top > 600) they sit 70 units over,
    squashed to 0.8 height (foot at 770 on a capital), as a hand flattens
    a capital's accent.
  - The circumflex and caron have a doubled tip; without it the midpoint
    rule rounds them into a breve.
  - The dots are handscript's 20-unit i-dot.
- **Conventions, by the standard forms.**
  - The i and j take above-accents on the dotless forms (ı U+0131 and ȷ
    U+0237 are registered too); į keeps its dot.
  - Unicode's "cedilla" on G K L N R is drawn as a comma below, and on g as
    a turned comma above: the Latvian forms every font uses. Ş ş Ţ ţ keep a
    real cedilla.
  - ď ľ ť Ľ write the caron as an apostrophe beside the stem (Czech/Slovak);
    Ď Ť keep a hat.
- **`sgbarred`** covers Ł ł Đ đ Ð Ħ ħ Ŧ ŧ Ø ø (a bar or slash), ď ľ ť Ľ, and
  Ŀ ŀ (middle dot): base plus strokes at fixed places and an extra advance.
  **`sgpairs`** covers æ œ Œ Ĳ ĳ ŉ: two glyphs overlapped by a fixed dx.
- **Authored in `sgsrc`.** Æ, ß (a long stem into a 3-like double bump),
  Þ þ, ð (a round d whose stem curls back, crossed), ĸ (a small K), Ŋ ŋ (n
  with the right leg hooked down), ſ (f without its bar).

- **Names.** The new glyphs use Adobe names, except ħ Ħ: `/hbar` was
  already the board-math ℏ, so they are `/hstroke` `/Hstroke`. The first
  identity run caught the clash (ℏ had turned into ħ); a SelfTest line now
  pins it.

## UTF-8 or Latin-1

- **The rule is per string.** A string that is well-formed UTF-8 (`sgutf8ok`:
  every byte from 80 up belongs to a sequence with lead byte C2-F4 and the
  right number of 80-BF continuation bytes) is decoded as UTF-8, exactly as
  before. Any other string is read as Latin-1, one byte one code point
  (`sgnameslatin1`). Every string that worked before decodes the same way,
  and so draws the same glyphs with the same per-index seeds.
- **The ambiguity.** A Latin-1 string whose bytes happen to form valid UTF-8
  is read as UTF-8. That needs a Latin-1 letter from C2 to F4 followed by
  one of the rare bytes 80-BF, such as `Ã©`. It does not happen in real
  words; it is the same trade-off as other UTF-8-with-Latin-1-fallback
  readers.
- **Per string, not per byte.** A mixed string such as `(caf\351 →)` is all
  Latin-1, so its `→` bytes become `â` and two C1 controls. Mixing the two
  encodings in one string is a bug in the caller, and a per-byte guess
  would hide it.
- **The strict error label is always readable.** A missing Latin-1 letter is
  labelled by its UTF-8 form (`(a\277)` gives `sg-missing-glyph:¿`, not a
  raw byte). A C1 control (80-9F) is labelled by number (`U+0086`).

## Evidence

- `scripts/identity_check.sh --loader lib/strokeglyphs.ps` covers every
  tracked file reaching strokeglyphs (mathkit, the chalk examples, the crayon
  gallery piece). All came out identical, with a `(é)` `/Strict false`
  control.
- `tests/strokeglyphs.rs`:
  - every covered code point draws from UTF-8, and up to U+00FF from its
    Latin-1 byte with the same width;
  - `(crème brûlée, naïve Zoë)` and its Latin-1 twin give the identical path;
  - Ghostscript builds the same accented path (UTF-8 and Latin-1) from the
    same seed.
- The SelfTest block `sgpath-reads-utf8-and-latin1-accented-letters` pins the
  convention and the error labels.
- Load time: `sginit`'s extra work is under 10 ms (pscat; not measurable
  against the base at `time -p` resolution).
- Specimen: `examples/strokeglyphs_accents.ps`, rendered to
  `docs/dev/design/drymedia/strokeglyphs_accents_specimen.png` (2x).

## Not done

- **mathkit takes UTF-8 only.** mathkit names its characters through
  `sgmetrics`, so a UTF-8 accented letter now lays out (`\text{café}`, and
  `pscat chalk` with it) where it used to raise `mk-unsupported-character`.
  A Latin-1 byte is still refused. mathkit slices a character's bytes by
  the lead byte (E9 claims three), and a Latin-1 reading of that slice
  would draw é and silently drop the two characters after it. So
  `sgmetrics` stays UTF-8 only, as it was before; a string that is not
  valid UTF-8 names `.notdef`. A review probe caught this
  (`(\351ab)` drew a lone é), and
  `mathkit_refuses_latin1_bytes_and_draws_utf8_accents` pins it. TeX's
  `\acute{}` and friends are a separate feature.
- **Latin-1 punctuation and symbols** are still missing: ¡ ¿ « » € £ ° and
  the rest.
- **Latin Extended-B** beyond ș ț, and Vietnamese, are still missing.
