# #252: Learn PostScript (site page)

## What shipped

- **`site/learn.html`** replaces #250's placeholder. Part 1 is a short,
  cited history (Joy and the Design System at Evans & Sutherland, JaM and
  Interpress at PARC, Adobe, the LaserWriter and PageMaker, the Linotronic,
  Type 1 / Type 3 / TrueType, Display PostScript and NeXTSTEP, the three
  LanguageLevels, EPS, PDF, today), with a reading list and 13 numbered
  sources. Part 2 is the tutorial: 20 short examples, each with a "Try it"
  link into the playground and its real output or render.
  Learn is site-only, as DOCS_PLAN.md section 2 decides; nothing is
  mirrored in `docs/guides/`.
- **Examples are files**: `site/learn/NN-name.ps`, plus `NN-name.out` for
  the ones that print. `scripts/site_examples.py` expands
  `<!--@example NAME "alt"-->` markers in `_site/` (source, Try it link,
  output from `.out` or the render at `assets/learn/NAME.png`), between the
  renders and `site_pages.py`. It fails on an unknown example, a drawing
  without alt text or render, or an example no page shows.
- **Playground**: `playground.html?example=PATH.ps` loads `examples/PATH.ps`
  instead of the default program (a strict name pattern; one folder level).
  A new output box under the controls shows what the program prints,
  updated every frame, hidden while empty. With no parameter the page looks
  as before.
- **Output capture** (the one interpreter change): `=`, `==`, `stack`,
  `pstack` and `print` now write through `Interp::write_stdout`. By default
  that is stdout, as before; `Interp::capture_stdout()` sends it to a buffer
  instead, capped at 64 KB with a truncation note, so a runaway
  `{ 1 == } loop` in a tab cannot grow memory without limit. The wasm build
  turns capture on in `ps_begin` and exports `ps_output_ptr`/`ps_output_len`;
  `pscat.js` exposes `ps.output` (documented on `javascript.html`).
  Without this, `3 4 add ==` printed nothing in the browser.

## Verification

- `tests/learn.rs`: every example runs in the CLI pscat with no stderr and
  prints exactly its `.out`; every drawing example inks its page; every
  example prints the same in Ghostscript (skipped when gs is absent); the
  markers in `learn.html` match the files one to one; capture collects and
  caps output.
- `scripts/check_learn_examples.mjs`, run by `build_site.sh` right after the
  wasm build: each example in `web/pscat.wasm` through `web/pscat.js`, no
  error, `ps.output` equal to `.out`, drawings not blank. This is what makes
  CI (site-check.yml, pages.yml) check the browser path: `ci.yml` does not
  install the wasm target, so `tests/wasm.rs` skips there.
- `check_site_links.py` now also checks `playground.html?example=` targets;
  `test_site_scripts.py` covers that and the expander.
- Renders of all eleven drawing examples were compared side by side with
  `gs -sDEVICE=png16m -r72`; they match.

## History sources

Every date and claim on the page was checked against the source it cites:
Warnock's 2018 IEEE Annals article (read in full), the Computer History
Museum's Adobe corporate timeline, its 2017 desktop publishing pioneer
transcript and its 2022 source-code article, the PLRM 3rd edition, the
Type 1 book, Microsoft's TrueType history, the EPSF 3.0 tech note, ISO
32000-1's introduction, Adobe's Type 1 end-of-support page and NeXT's
Release 3 programming summary. Left out for lack of a source I could check:
exact LaserWriter announce/ship days and price, PageMaker's month, Ghostscript's
first-release date, the Quartz-replaces-Display-PostScript story, and the
"blue book"/"red book" nicknames. The CHM timeline dates the Type 1 open
standard to 1989 and the spec book to 1990; the page gives both.

## Newcomer pass

Every operator an example uses is introduced in the prose before or beside
it (`rectfill`/`rectstroke` and `rlineto` got a sentence each after the first
read-through). Outputs that differ between interpreters were avoided:
`countdictstack` (pscat 2, gs 3) and non-terminating reals were dropped.
The landing page's Learn card no longer says the tutorial is "on its way".

## Follow-ups

None filed. A drawing example's canvas in the playground is drawn at its
point size (300 x 200) without upscaling, as for the playground's other
small examples; that is the playground's existing behaviour.
