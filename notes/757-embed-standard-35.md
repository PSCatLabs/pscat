## The whole standard 35 compiled in (issue #757, 2026-10-25)

Owner decision on 2026-10-25: every install gets the LaserWriter 35. Before this, only the
twelve Liberation faces were compiled in; the six TeX Gyre families were the `texgyre` pack
and the URW Symbol/ZapfDingbats faces were part of `extras`. That split came from grouping by
licence (#563, #599), not size, and a bare install (fractal_clouds, #727) drew Palatino as
Times.

**Where the files are.** `fonts/catalog/TeXGyre-*` and `fonts/catalog/URW-symbols` moved to
`fonts/base35/`, with their licence files. They had to leave `fonts/catalog/`:
`scripts/check_licences.py package` forbids anything under it in the crate, and
`fonts/packs.json` must match `fonts/catalog/` exactly. Every font file is byte-identical
to the copy in the `fonts-2026.10` packs (checked by SHA-256 against the released archives).

**How they load.** `font::EMBEDDED` is a `(stem, include_bytes!)` table. These faces are not
`BUILTINS`. A builtin gets StandardEncoding and `FontName` from its `ps_name`, which would
take Symbol's PLRM encoding away and change `FontName` for every face. Instead
`font::embedded_fid` registers a face through `register_face` the first time a name needs it,
under the pack file's stem. That is exactly how a disk file of that name loaded, so the
encoding, `FontName` and FID sequence are the same, and so are the pixels. `resolve` order:
builtins, then disk (overrides, then installed packs), then embedded, then the #728
stand-in. Disk first keeps user overrides winning, and anyone with the old pack installed
stays on today's code path. wasm takes the same embedded step after `register_font_bytes`
faces.

**Packs.** The issue offered dropping the faces from the packs or keeping them as
overrides. I dropped them: the files can live in only one of `fonts/catalog/` (packs) and
the crate. `texgyre` is retired (`fontpacks::RETIRED_PACKS`):
- `pscat fonts install --pack texgyre` prints one line saying the faces are built in and
  exits 0, fetching nothing, so scripts keep working;
- a plain install from an index that still lists it skips it with the same line. That
  includes the pinned `fonts-2026.10`, checked offline against the real released index:
  extras and cjk-thai install as `pinned`.

`fonts-2026.10`'s `extras` still carries the URW files. Installed, they are the same bytes
and win as disk files do. The next `fonts-YYYY.MM` built from the new table drops them, and a
pscat release after it re-pins with `font_packs.py pin`. That is an owner step.

**Stand-ins.** `stand_in_for` lost the Times/Times-Italic branches and the Symbol glyph
caveat in the note. Only `extras` (Courier for the monospaced faces, Helvetica otherwise)
and `cjk-thai` remain. The libraries' rejection tests (`lettering`, `headline`, `jersey`)
moved to extras faces; Palatino and ZapfChancery now pass them, as real faces.

**`--fonts` / `--capabilities`.** The 23 stems are listed as `builtin`, and aliases to them
(the Palatino names, implicit `-Regular` names) report availability `builtin`. A disk file
with the same stem shadows the embedded entry, which is then listed as `catalog`.

**Licences.** The package expression is now `(MIT OR Apache-2.0) AND OFL-1.1 AND LPPL-1.3c`
in `Cargo.toml`, `npm/package.json` and `licenses/assets.json`. LPPL-1.3c is the SPDX id
#563 used for the GUST Font License. The new term is also in the pscat exception in
`deny.toml` and in `about.toml`'s accepted list (`cargo deny check licenses` and
`cargo about generate --fail` pass). The crate, the release archive (`package_release.sh`,
which now also checks it) and the npm tarball (`build_npm.sh`'s exact file list) ship each
`GUST-FONT-LICENSE.txt`, plus URW's `LICENSE.md`, `LICENSE.OFL` and `PROVENANCE.md`: the same
files the packs carried. URW's AGPL/LPPL texts stay in the repo only, as before.
THIRD_PARTY_NOTICES.md has a "Standard 35 faces (compiled in)" section.

**Sizes** (before → after, `7dcd6c7` → this branch, macOS arm64):

| | before | after | delta |
|---|---|---|---|
| `cargo package --list` | 132 files | 164 files | +32 |
| `.crate` (verified build) | 3,364,084 B | 5,168,916 B | +1,804,832 B (+1.72 MiB) |
| release `pscat` | 10,602,080 B | 14,058,768 B | +3,456,688 B (+3.30 MiB) |
| release `pscat-mcp` | 9,080,288 B | 12,519,552 B | +3,439,264 B (+3.28 MiB) |
| release archive (`.tar.gz`, both binaries) | 10,187,207 B | 13,821,468 B | +3,634,261 B |
| `pscat.wasm` | 7,914,083 B | 11,304,816 B | +3,390,733 B |
| npm tarball, packed | 3,528,765 B | 5,348,362 B | +1,819,597 B |
| npm tarball, unpacked | 8,016,677 B | 11,429,126 B | +3,412,449 B |

Each binary carries its own copy, so the archive grows by about twice the compressed fonts.

The raw fonts are 3,410,892 B. gzip -9 takes them to 1.78 MB and zstd -19 to 1.18 MB. Storing
them compressed would have saved about 2.2 MB of binary, at the cost of a decompressor on
wasm and a first-use inflate. Both owner limits (crate 8 MB, binary +4 MB) hold without it,
so the bytes are stored plain. `build_npm.sh`'s budgets went to 6 MiB packed and 13 MiB
unpacked.

**#750 after this.** The standard 35 no longer substitute on wasm, so #750's example
(`/Palatino-Italic`) is moot. Still open: on wasm a display or CJK/Thai face that was not
`addFont`ed becomes Helvetica silently. Its acceptance should name `/Bangers`.
