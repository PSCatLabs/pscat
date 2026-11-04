## When to use pscat: a tool-choice guide and pointers (issue #994, 2026-11-04)

Added `docs/guides/when-to-use-pscat.md`, a guide that answers "why use pscat
when I can ask an image model?" head-on, and `site/when-to-use.html`, its
summary on the site with the same decision table. The site nav gets a
`when-to-use` slot ("When to use", second after Home), and the front page and
`guides.html` get a card. Short pointers link the guide from README.md,
AGENTS.md, CLAUDE.md, the three plugin skills (absolute GitHub URL, since an
installed plugin has no repo-relative docs), `docs/guides/README.md`,
`docs/guides/agents.md`, `docs/dev/README.md` and `site/agents.html`.

The argument: image models win on photorealism, faces, light, painterly mood
and breadth, and on editing existing pictures. pscat wins where output must be
exact and checkable (text, QR, charts, math, layout, print-ready vectors),
reproducible and editable as source, and local and free per image. Its paint
media are deterministic, editable, vector and composable with exact parts.
pscat also lets a model with no native image output (Claude, as of the date
written) draw and look at real pictures through the plugin or MCP server; the
owner asked for that point mid-run, and then for two more sections: PostScript as a
program versus SVG/HTML as markup (with what SVG/HTML do better), and the
deterministic workflow (render = function of source + seed, the WebAssembly
build). The hosted MCP server (#689) is not shipped and is not claimed. LLM-written SVG and matplotlib are treated
fairly: SVG is also diffable vector source, and matplotlib is the better
choice when the data is already in Python.

Honesty rules followed: no benchmarks; claims about other tools are dated
"as of November 2026" and stated as general; seeds are conceded to
open-weights models (the difference claimed is the artifact, source vs image);
"free" is conceded to local open-weights models (the difference claimed is no
GPU); "offline" is qualified by the one-off font pack and plugin downloads;
QR only, with "no 1D barcodes" listed as a weak spot.

### Claim → evidence

| Claim in the guide | Evidence |
|---|---|
| Standard 35 compiled in; 67-face catalog incl. KR/JP/Thai as font packs | `docs/guides/fonts.md` intro; CLAUDE.md stage 22 |
| Native QR encoder, versions 1–40, L/M/Q/H, vector | `docs/guides/qr-codes.md`; `lib/qr.ps` |
| QR PNG/SVG/PDF exports decode with two independent decoders | `docs/dev/design/QR_CODE_TESTING.md` (#973); `tests/qr.rs`, `tests/qr_corpus.rs`, `tests/qr_card.rs` |
| No 1D barcodes | `grep -i barcode lib/*.ps` finds none |
| `pscat chalk` LaTeX subset; `lib/mathkit.ps` | `pscat chalk -o chalk.png 'x^2'` ran; `lib/mathkit.ps` header |
| dataviz bar/line/area/scatter/pie, graph.ps plots | `docs/guides/charts.md` |
| `--svg`, `--pdf`, `--dpi`; PNG records dpi; `%%Title:`/`%%For:` → PDF metadata | `pscat --help`; `docs/guides/getting-started.md` |
| Templates: card, letter, certificate, invitation, poster; typewriter | `docs/guides/style-packs-and-templates.md`; `pscat --help` (`typewrite`) |
| Business card with a QR code | `gallery/pscat_card.ps` (#974) |
| Same seed same picture; artkit same pixels on macOS/Linux/browser | `docs/guides/artkit.md` lines 5-11; the interpreter-wide basis is #738 (below) |
| Watercolor wash and paper deterministic under `/Seed`; wash is vector geometry | `docs/guides/painting.md`, "Watercolor: pkwash and pkpaper" |
| `--sweep-seed`, `pscat sheet` | `pscat --help`; ran `--sweep-seed 1:2 --contact-sheet` |
| `--lint-strict` flags a blank page, exits non-zero | ran on a blank `showpage` file: `[blank-page]`, exit 1 |
| MCP returns rendered PNG with errors | `site/agents.html` tool table; `docs/guides/agents.md` |
| Reads a JPEG, turns it into an engraving | `pscat --help` (`photo-etch`); `docs/guides/printmaking.md` |
| Code is MIT or Apache-2.0 | README "Licence"; `Cargo.toml` `license` |
| PostScript has loops, procedures, recursion, dicts; artkit L-systems, scatter, tilings, fractals, brushes | PLRM; `docs/guides/artkit.md` feature list |
| One program → `--png`, `--pdf`, `--svg`; multi-page PDF | `pscat --help`; `docs/guides/getting-started.md` "Multi-page output" |
| Same input → same pixels every run | `tests/drykit.rs` determinism tests; `docs/guides/dry-media.md` ("same bytes, every time" for `pscat chalk`) |
| Seeded renders match on macOS, Linux and the browser build, rare thin-curve exception | `notes/738-fmath-one-libm.md` (0 differing results/pixels after #738); `docs/guides/artkit.md` lines 7-11 |
| Test suite pins rendered output | `tests/golden.rs` (vs Ghostscript), `tests/render.rs`, per-library pixel tests |
| WebAssembly build renders client-side; npm `pscat`; playground | `src/wasm.rs`, `web/pscat.js`, `scripts/build_wasm.sh`, `site/playground.html`; README install block |
| Evening Stream, Alpine Lake are painted with paintkit | `gallery/manifest.json` libraries |

Deferred: the hybrid showcase is #995. The guide links the issue and names its
planned path `docs/guides/pscat-with-image-models.md` in code, not as a link,
so no link breaks before #995 lands; #995 should turn it into a link and link
back.
