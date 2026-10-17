## Licence files, asset audit and package metadata (issue #563, 2026-10-16)

The repository had no licence. It now has one, with every tracked file
accounted for. The decisions come from the "Resolved spec" at the top of
#563.

**What shipped**
- `LICENSE-MIT`, `LICENSE-APACHE` (the code), `LICENSE-OFL` (the embedded
  font data: the Liberation and Courier Prime copyright lines, then the
  OFL-1.1 text), and `LICENSE-CC0` (verbatim CC0 1.0 legal code, so GitHub can
  detect it; its scope is stated in the notices).
- `THIRD_PARTY_NOTICES.md`, a readable inventory: the component table, the
  CC0 boundary, exceptions, the provenance of the encoding tables, and all 51
  font catalog families with licence and copyright.
- `licenses/assets.json`, the machine-readable form: ordered path globs, and
  the first match wins. A throwaway check matched all 1018 tracked files
  against it. Every entry is used. Every image or font that falls to the
  code licence was looked at (pscat renders, `gs` renders, synthetic test
  JPEGs, the owner's shirt photo in `docs/archive/fonts/`). #601 owns turning
  that into CI.
- `Cargo.toml [package]` now has `license = "(MIT OR Apache-2.0) AND OFL-1.1"`,
  repository, homepage, documentation, readme, keywords, categories, authors
  and `rust-version = "1.88"`. 1.88 is the floor because of let chains,
  checked with `cargo +1.88 check --all-targets --locked`. It also has an
  `include` allowlist:
  - the published crate is `src/`, `build.rs`, `lib/**/*.ps`, the twelve
    Liberation faces, `fonts/LICENSE` and `fonts/README.md`, `README.md`, the
    MIT, Apache and OFL licences, the notices and the manifest;
  - that's 109 files, 3.1 MiB compressed, and `cargo package` verifies the
    build;
  - tests and benches are dropped from the packaged manifest, with a warning,
    as usual for an allowlist.
- `fonts/catalog/URW-symbols/`: the mirror's `LICENSE.md`, `LICENSE.OFL` and
  `LICENSE.LPPL` (so the statement's links resolve), plus `PROVENANCE.md`.
  The original `LICENSE`/`COPYING` (AGPL) stay as the other options of the
  triple licence. Neither font changed.
- A Licence section in the README, and the stale "AGPL" and "dumped from
  Ghostscript" wording corrected in `fonts/catalog/README.md`,
  `docs/dev/FONTS.md`, `docs/guides/fonts.md` and the comment in
  `src/encodings.rs`.

**The encoding tables**
- I didn't claim they were extracted independently. Instead:
  - SymbolEncoding was compared against PLRM 3rd ed. Appendix E.12, parsed
    from Adobe's PDF. All 189 names and octal codes match, and everything else
    is `.notdef`.
  - DingbatsEncoding: PLRM 3 has no table for it. The 188 codes we encode
    match Adobe's `zdingbat.txt`. Adobe also encodes `\200`–`\215`, which are
    `.notdef` here and in gs 10.07.1. #605 decides whether to add them, since
    that would change rendering and diverge from the oracle.
- The comment now records both facts.

**URW**
- Both `.otf` files were last touched by Artifex commit `79bcdfb` (2016-07-12).
  That predates the 2017-09-01 triple-licence commit `a8265008`, so the grant
  covers these bytes.
- The 2017 `LICENSE.md` only lists the three licences. The "choose any one"
  wording is the mirror maintainer's 2026 clarification. `PROVENANCE.md`
  quotes both and treats the 2017 text as the operative one.

**CC0 audit (gallery/, examples/, site/)**
- Programs with inlined `lib/` code, where those parts stay MIT OR
  Apache-2.0: `gallery/hortus.ps`, `infinite_descent.ps`, `ripple_range.ps`
  and `woven_labyrinth.ps` (their "toolkit subset" sections), and
  `examples/dataviz.ps` and `graphing.ps` (`showctr`). Found by matching
  `/name {` definitions against `lib/` procedure names, plus a grep for
  "inlines".
- Lincoln's reference photo is public domain.
- `gallery/references/mochi.jpg` and `art/` were first held back as
  `NOASSERTION` pending the owner (#606). Owner decision, 2026-10-16: `art/`
  is CC0-1.0. The photo stays `NOASSERTION` until the owner confirms they took
  it; the painting made from it is CC0 either way.
- The memos' names and offices are invented (#214, #215, #590), and the poem
  in *Fugitive Pigments* is original. The site's HTML/CSS/JS,
  `gallery/manifest.json` and `show.sh` are code.
- `docs/dev/design/*/refs` and the panels built from them keep their sources'
  terms, per the existing REFERENCES.md files.

**Not done here**
- CI enforcement (cargo-deny, the font-directory check) is #601.
- `cargo-about` dependency notices are #602.
- npm metadata comes with the npm package.
- Font pack manifests are #599.
- The CONTRIBUTING inbound = outbound terms are #603.
