# fonts/catalog — the runtime font catalog

A wide shelf of libre faces that `findfont` loads **from disk on first
use** — nothing here is compiled into the binary. The standard 35 are:
the twelve Liberation faces in `fonts/` and the TeX Gyre and URW faces
in `fonts/base35/` (issue #757). Drop a `.ttf`/`.otf`/`.ttc` into any subdirectory
here and `/<FileStem> findfont` finds it; `/<Family> findfont` falls
back to `<Family>-Regular`. `pscat --fonts` lists everything reachable.
(`.ttc` collections always load face index 0 — verified against a
synthetic two-face collection, not just assumed; no collection is
bundled today.)

Font lookup checks every `fonts/catalog/` directory the resolver
finds, then the installed font packs (`src/fontpacks/mod.rs` has the
order; `docs/guides/getting-started.md` the user-facing version). This
directory is the source of the packs: `fonts/packs.json` assigns each
family to a pack (`extras`, `cjk-thai`), and a new family
must be added there too, or `cargo test` fails. Installed pscat builds
get these faces with `pscat fonts install`; see
`docs/guides/fonts.md`, "Font packs". Every family directory carries its
own license text alongside its font files. Licenses here are the SIL
Open Font License 1.1 (OFL) and Apache 2.0. All are freely
redistributable; none restrict rendering or embedding in documents.
`THIRD_PARTY_NOTICES.md` at the repo root has the full inventory.

## Display, text, and mood faces (one style each)

All from the Google Fonts collection; each directory carries the
family's `OFL.txt` or Apache `LICENSE.txt`.

| Face | Genre / mood | License |
|---|---|---|
| EBGaramond | old-style garalde serif (Garamond; aliased `/Garamond`) | OFL |
| LibreBaskerville | transitional serif (aliased `/Baskerville`) | OFL |
| PlayfairDisplay | didone display serif | OFL |
| Lora | contemporary text serif | OFL |
| Cinzel | classical Roman capitals (Trajan spirit) | OFL |
| Poppins | geometric sans | OFL |
| Jost | Futura-spirited geometric sans | OFL |
| Oswald | condensed gothic sans | OFL |
| AtkinsonHyperlegible | high-legibility humanist sans | OFL |
| ZillaSlab | contemporary slab serif | OFL |
| AlfaSlabOne | fat-face slab display | OFL |
| Graduate | collegiate block slab, chamfered numerals (the varsity jersey face; issue #167) | OFL |
| Anton | tall condensed athletic gothic (hockey / pro jersey numbers; issue #167) | OFL |
| Chewy | soft hand-drawn 60s/70s display, flared-leg R, slanted-top l (best stand-in for the Keepin' it Real shirt face) | Apache 2.0 |
| BagelFatOne | blobby ultra-heavy rounded display (Cooper-Black spirit, heavier alternative) | OFL |
| JetBrainsMono | modern coding monospace | OFL |
| GreatVibes | formal copperplate script | OFL |
| Pacifico | 1950s brush script | OFL |
| Caveat | casual handwriting | OFL |
| HomemadeApple | flowing pen handwriting | Apache 2.0 |
| BebasNeue | tall condensed poster caps | OFL |
| AbrilFatface | heavy didone advertising face | OFL |
| Monoton | inline neon-sign display | OFL |
| Limelight | art-deco marquee | OFL |
| Bungee | chromatic urban signage | OFL |
| UnifrakturMaguntia | blackletter fraktur | OFL |
| PirataOne | condensed blackletter | OFL |
| Rye | western circus/wanted-poster | OFL |
| Creepster | horror-movie lettering | OFL |
| PressStart2P | 8-bit arcade pixels | OFL |
| VT323 | glowing-terminal mono | OFL |
| SpecialElite | worn typewriter | Apache 2.0 |
| CourierPrime | typewriter Courier with slab serifs; the source of the `/Typewriter` font's outlines (`lib/typewriter.ps`, issue #211) | OFL |
| Orbitron | sci-fi geometric display | OFL |
| Audiowide | streamlined techno display | OFL |
| StardosStencil | stencil serif | OFL |
| Bangers | comic-book shout | OFL |
| ComicNeue | the casual comic sans, done right | OFL |
| MedievalSharp | carved medieval lettering | OFL |
| PermanentMarker | felt-tip marker | Apache 2.0 |

Sources: `github.com/google/fonts` (`ofl/` and `apache/` trees).

## Korean, Japanese, and Thai (Unicode-mode)

Hangul and kanji don't fit in the 256-slot Encoding model every other
face here uses, so these three get a documented deviation instead:
`show` decodes their strings as UTF-8 and maps codepoints straight to
glyphs (see docs/dev/FONTS.md's "Unicode-mode catalog faces" addendum). Literal
Korean/Japanese/Thai text in a `(...)` string just works — no custom
Encoding array needed. Both are variable fonts pinned to their Regular
weight (`wght` 400) at load time rather than the file's own default
instance, which is Thin for these two.

| Face | Script | License |
|---|---|---|
| NotoSansKR | Korean (Hangul + Latin) | OFL |
| NotoSansJP | Japanese (kana + kanji + Latin) | OFL |
| NotoSansThai | Thai (+ Latin) | OFL |

Source: `github.com/google/fonts` (`ofl/notosanskr`, `ofl/notosansjp`,
`ofl/notosansthai`).

A second Korean face rides the same Unicode-mode mechanism, for a
handwritten/brush look rather than plain sans:

| Face | Script | License |
|---|---|---|
| NanumBrushScript | Korean, brush calligraphy | OFL |

Source: `github.com/google/fonts` (`ofl/nanumbrushscript`). Static (not
variable), so no default-instance pinning needed.

For the *programmatic* faces — glyphs that are PostScript procedures,
not outlines — see `lib/fonts/`.
