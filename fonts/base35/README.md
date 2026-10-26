# fonts/base35 — the rest of the standard 35, compiled in

The TeX Gyre and URW faces behind the 23 standard PostScript names the
Liberation faces in `fonts/` do not cover. Since issue #757 they are
compiled into pscat with `include_bytes!` (`EMBEDDED` in `src/font.rs`),
native and wasm alike, so every install has all 35 with no font pack.
Before that they were the `texgyre` font pack and part of `extras`; the
bytes are the same as in the `fonts-2026.10` packs.

With these and the Liberation faces in `fonts/`, every one of the
classic LaserWriter 35 names resolves to a metric-compatible libre face
(the alias table lives in `src/font.rs`):

| Directory | Standard names covered | Face | License |
|---|---|---|---|
| `TeXGyre-pagella` | Palatino-Roman/-Bold/-Italic/-BoldItalic | TeX Gyre Pagella (GUST) | GFL |
| `TeXGyre-bonum` | Bookman-Light/-Demi/-LightItalic/-DemiItalic | TeX Gyre Bonum (GUST) | GFL |
| `TeXGyre-schola` | NewCenturySchlbk-Roman/-Bold/-Italic/-BoldItalic | TeX Gyre Schola (GUST) | GFL |
| `TeXGyre-adventor` | AvantGarde-Book/-Demi/-BookOblique/-DemiOblique | TeX Gyre Adventor (GUST) | GFL |
| `TeXGyre-chorus` | ZapfChancery-MediumItalic | TeX Gyre Chorus (GUST) | GFL |
| `TeXGyre-heroscn` | Helvetica-Narrow (+Bold/Oblique/BoldOblique) | TeX Gyre Heros Cn (GUST) | GFL |
| `URW-symbols` | Symbol, ZapfDingbats | URW Standard Symbols PS, D050000L | OFL (chosen from AGPL/LPPL/OFL) |

The URW faces get their own encodings (SymbolEncoding /
DingbatsEncoding, `src/encodings.rs`; provenance in
`THIRD_PARTY_NOTICES.md`).

A font file of the same name on disk (a `fonts/catalog/` override or an
installed pack) is used instead of the compiled-in copy; the files here
are never read at runtime.

Licences: the TeX Gyre faces are under the GUST Font License
(`GUST-FONT-LICENSE.txt` in each directory; SPDX `LPPL-1.3c`, with a
request that derived works be renamed). The URW symbol faces are taken
under OFL-1.1 from URW++'s AGPL/LPPL/OFL triple licence
(`URW-symbols/LICENSE.md`, `LICENSE.OFL`, `PROVENANCE.md`; the AGPL and
LPPL texts beside them are upstream's, kept verbatim). The crate, the
release archives and the npm package ship the GUST text, the OFL text,
`LICENSE.md` and `PROVENANCE.md`. `THIRD_PARTY_NOTICES.md` at the repo
root has the full inventory.

Sources: TeX Gyre from CTAN (`fonts/tex-gyre`), URW faces from
`github.com/ArtifexSoftware/urw-base35-fonts`.
