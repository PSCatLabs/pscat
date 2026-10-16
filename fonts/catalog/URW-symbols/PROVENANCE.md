# URW Standard Symbols PS and D050000L: provenance and licence

The two faces here back the standard PostScript names `Symbol`
(`StandardSymbolsPS.otf`) and `ZapfDingbats` (`D050000L.otf`). pscat
distributes them under the **SIL Open Font License 1.1**, one of the three
licences URW++ offered for the Core 35 fonts in 2017 (AGPL-3.0 with a
font-embedding exception, LPPL-1.3c or OFL-1.1, the licensee's choice). This
file records why we think that grant applies to these exact files, and what we
could not confirm. Decided in #563.

## The files

| File | SHA-256 | Version |
|---|---|---|
| `StandardSymbolsPS.otf` | `df570efda2df425dbfc004e4e5f77c55ca1b47f8ff28ad60dfad0fccf422b0bd` | 2.0 |
| `D050000L.otf` | `a7bd946b69ae526328f26b5339fb31057dd40950d7b60598f36b2bd06542f105` | 2.0 |

Both are byte-identical to the copies in the triple-licensed repository below
(checked on 2026-10-16), and to Artifex's AGPL distribution,
[`ArtifexSoftware/urw-base35-fonts`](https://github.com/ArtifexSoftware/urw-base35-fonts),
which is where pscat's copies came from (Stage 18, commit `c2efc65`). Switching to the OFL changed only licence
files. No glyph, metric or encoding changed, and nothing renders differently.

## The upstream chain

1. **URW++ to Artifex.** URW++ Design & Development delivered the Core 35
   fonts to Artifex Software, which committed them to its font repository under
   AGPL-3.0 with the font exception (the `LICENSE` and `COPYING` files here).
   Both of our files were last changed in commit
   [`79bcdfb34fbce12b592cce389fa7a19da6b5b018`](https://github.com/twardoch/urw-core35-fonts/commit/79bcdfb34fbce12b592cce389fa7a19da6b5b018)
   by Chris Liddell of Artifex, 2016-07-12, "URW++ update to base 35 from June 2016". No
   later commit touches either file, so the 2017 licensing below covers these
   exact bytes, not only a later update.
2. **URW's own repository, 2017.** The history continued in URW's GitHub
   repository `URWTypeFoundry/Core_35`. There, on 2017-09-01, Adam Twardoch
   committed the triple licence:
   - [`a8265008f73e13a714450b3a448999aa9f3192ec`](https://github.com/twardoch/urw-core35-fonts/commit/a8265008f73e13a714450b3a448999aa9f3192ec),
     "3-licensing: AGPL, LPPL, OFL". It added `LICENSE.md`, `LICENSE.OFL`,
     `LICENSE.LPPL` and `README.md`, and renamed the AGPL exemption file.
   - Follow-ups the same day: `9171643071`, `3a03cc9685`, `68062f21a3`,
     `72035a76e4` ("3-licensing (AGPL LPPL OFL)") and `1b5267a9a1`
     ("Update README.md").
3. **The mirror.** `URWTypeFoundry/Core_35` has since been deleted (it returns
   404). Its full history survives in
   [`twardoch/urw-core35-fonts`](https://github.com/twardoch/urw-core35-fonts)
   ("Fork of defunct https://github.com/URWTypeFoundry/Core_35"), which also
   merged `URWPP/Core_35` in 2020
   (`f26e9ba646428548c7600668f073848bbcefd78f`). The commit links above point
   into the mirror.

## The licence files here

| File | What it is |
|---|---|
| `LICENSE.OFL` | The OFL-1.1 text with URW++'s copyright line, from the mirror. SHA-256 `5defa64c721ef9a2c590e044582789791f4483fd18471331b543cfcdf7741cc3`. It names no Reserved Font Name. **This is the licence pscat distributes the fonts under.** |
| `LICENSE.md` | The mirror's licence statement, as of its `main` branch (2026-10-16). |
| `LICENSE` | The AGPL font-exception notice from the Artifex distribution, as originally shipped here. |
| `COPYING` | The AGPL-3.0 text from the same distribution. |
| `LICENSE.LPPL` | The LPPL-1.3c text, from the mirror, so that `LICENSE.md`'s links resolve. |

`LICENSE`, `COPYING` and `LICENSE.LPPL` are the other two options of the
triple licence, kept so the record is complete. They are not the terms pscat
applies, and pscat ships no AGPL font pack.

**The wording changed over time.** The 2017 `LICENSE.md` (commit
`a8265008`, the only 2017 commit to touch that file) read, verbatim:

```markdown
# License information for URW Core 35 Fonts, Version 2.00

The fonts and related files in this directory are distributed under the following licenses:

* [GNU Affero General Public License, Version 3](./COPYING) with an [exemption](./LICENSE)
* [The LaTeX Project Public License, Version 1.3c](./LICENSE.LPPL)
* [SIL Open Font License, Version 1.1](./LICENSE.OFL)

Copyright 2013,2014,2015 by (URW)++ Design & Development
```

The copy here has the wording that the mirror's maintainer added in 2026
(commit `85a07f255f7e47f495d4e9967b9a040194fcdd55`, "Clarify license
information, explicitly add exemption"): "Each licensee may choose any one of
these licenses to apply". That clarification comes from the mirror, not from
URW++. **The 2017 text is the operative grant.** Listing three licences for
the same files is a disjunctive offer, which is how the clarification reads it
too.

## What we could not confirm

- There is no surviving first-party URW++ statement. A deleted official
  repository is not the same as a live licence page. The hashes show the files
  are identical; they do not prove who had authority to license them.
- Fedora's `urw-base35-fonts` package still uses the Artifex AGPL distribution.
  Others rely on the OFL grant: MuseScore's Edwin font, an OFL derivative of
  C059, and Fedora's `edwin-fonts` package.

The owner accepted this residual risk in #563. A short written confirmation
from URW++ or Adam Twardoch would strengthen the record and can be added here.
It is optional and does not block a release.
