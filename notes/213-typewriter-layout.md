# #213: tw-type, memo layout, scripts/typewrite.sh

Epic #209. Builds on #211 (font) and #212 (ribbon ink); read
`notes/211-typewriter-font.md` and `notes/212-ribbon-ink.md` first. The design
is `docs/TYPEWRITER.md`; "As built (#213)" is at the end of its section 9.

## What shipped

- `lib/typewriter.ps`, section "layout":
  - `tw-type`, `tw-linecount`, `tw-pageheight`: three tagged procedures (the
    catalog reads their `@param`s, every one with a unit).
  - Private code in `TWState` under the prefix `ly` (artkit owns `tw*`).
- `scripts/typewrite.sh`: text file, stdin or `--text` to PNG or PDF; bundle
  aware like `handwrite.sh`; also in `scripts/package_release.sh`.
- Catalog: a hand-written `Typewriter` Type3Face row in `src/capabilities.rs`
  (build.rs rejects `@kind: Type3Face`). Its load sequence runs the outlines
  first. `tests/capabilities.rs` gains `lib/typewriter.ps` in the Type 3
  on-disk list, loads the outlines before the face, and leaves Type 3 rows out
  of the migrated-name check (a face is in FontDirectory, not userdict).
- Docs: `docs/guides/fonts.md` (the typewriter section; the sentence it ended
  on was cut off by #248), the `pscat` and `psart` skills, `HANDOFF.md`.
- `examples/typewriter_memo.txt` and its renders `docs/typewriter_memo.png`
  (pscat, 200 dpi) and `docs/typewriter_memo_gs.png` (gs, from `--ps`).
- Tests: `tests/typewrite.rs` (13), `%%SelfTest` blocks `tw-type-guards` and
  `tw-linecount-wraps` in the library.

## Acceptance criteria

1. **`tw-type` and `tw-linecount` always agree on line breaks.** Both run one
   layout (`lyLayout`, which fills a list of `[a b col0]` line records);
   `tw-linecount` returns its length and `tw-type` draws from it. The test
   `tw_linecount_agrees_with_what_tw_type_types` checks it over 5 texts x 4
   option sets, through the carriage position `tw-type` leaves (first
   baseline minus lines x step).
2. **A multi-paragraph memo renders through `scripts/typewrite.sh` in pscat
   and gs.** `ghostscript_types_the_memo_the_script_writes` writes the driver
   with `--ps`, runs it under `gs -dNOSAFER` and checks the rows with ink
   against the script's own pscat PNG. Local only (CI has no gs). The
   committed renders are the same memo in both.
3. **Tests cover wrapping, tab stops, strikeover markup and determinism.**
   The placement tests compare a typed page with the same characters typed
   by hand (`moveto`/`show` in the same frame), which holds because every
   random draw is keyed by the cell and strike index.
4. **Catalog and skill docs.** Done as above.

## Decisions made here

- **One options dict.** The layout keys and every `tw-setup` key go in the
  same dict. `lySplit` reads the layout keys and hands the rest to `tw-setup`,
  which checks them; a key in neither set is `tw-unknown-option`. Layout keys
  do not persist from call to call; `tw-setup` keys do, as before.
- **Guards.** Every layout guard ends `TWState` first, as `tApply`'s do. `tw-type`
  calls `tw-setup` after an `end`, so a rejected key never leaves `TWState`
  on the dictionary stack (`%%SelfTest` checks `countdictstack`).
- **Measuring changes nothing.** `tw-linecount` and `tw-pageheight` read
  `/Pitch` and `/Size` from the dict (else the current values) and never apply
  options or set the font. `tw-pageheight` exists so `--height fit` is computed
  in PostScript, with the same geometry as `tw-type`, not duplicated in awk.
- **Lines.** The text becomes three parallel strings (character, attributes,
  struck-over character), one byte per cell. Wrapping is greedy by word on
  columns; whitespace where a line breaks is dropped, a word wider than a
  line is split, leading whitespace on a paragraph's first line is kept. A
  final newline ends the last line instead of starting a blank one; empty
  text is zero lines.
- **Spacing** rounds to the platen's 6pt half-lines. #211's line layer already
  gives each line its carriage-return offset, advance error and tilt, keyed by
  the frame row, so layout adds none; it only has to place text on whole rows.
- **Strike indices.** Text at the caller's `/Strike`, x-outs and overtypes at
  +1, underlines at +2; restored afterwards. So an underscore under an
  underscore, and an x over an x, are second blows.
- **Markup**: `<u>`, `<x>`, `<o>old|new</o>`; no escapes, other `<` is
  literal. `<o>` is one unbreakable token as wide as the longer side.
- **Pages.** Overflow continues after a `showpage` (none after the last page).
  Page *p*'s frame sits `p x lines-per-page x step` above the page, so row keys
  keep counting and no two pages repeat the same variation. `/PageStart`
  runs before every page, the first included, with `TWState` ended around it,
  so an artist (or #214) paints paper without `tw-type` clearing it.
- **Memo.** A header string is built from `/Memo` and prepended to `/Text`:
  centred underlined title, fields padded to column 10 and hung there, an
  underscore rule (full width), a blank line. The hang is per paragraph, so
  long subjects wrap under their text.
- **Page numbers**: `/bottom` "- n -" centred on every page; `/top` "n" right
  aligned from page 2. Each is typed on the grid.
- **Non-ASCII.** The library gives bytes above 126 one blank cell each (and
  `tw-linecount` counts them the same way). `typewrite.sh` first turns
  typographic quotes, dashes, the ellipsis and the bullet into ASCII and any
  other character into `?`. macOS `iconv //TRANSLIT` does not work, hence the
  sed table.
- **Script.** `PSCAT_BIN` overrides binary lookup (tests pass
  `CARGO_BIN_EXE_pscat`, since `cargo test --profile ci` builds under
  `target/ci`, not `target/release`). `--ps PATH` writes the driver. `--reflow`
  joins a paragraph's lines (a paragraph ends at a blank line); without it
  newlines break lines, as for a typist.
- **Gotchas found.** A procedure value (`/PageStart`) stored under a name runs
  when the name is looked up: keep it on the operand stack in `lySplit`, and
  fetch it with `/lyPS load`.

## Not done

- No ink or paper effects; #214 owns them (`/PageStart` is the hook).
- No proportional faces, justification or hyphenation, and no form feed.
- Typing from the MCP server.
- A numbered list's continuation lines return to the margin; no hanging
  indent key beyond the memo's.
