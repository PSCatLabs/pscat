## Pack fonts stand in on a bare install; the skills' code runs in CI (issues #727, #728, #729, 2026-10-25)

Found by building ArtColony/fractal_clouds on pscat 0.6.2 with no font packs: the skills
said the LaserWriter 35 "always resolve" and that unknown names substitute Helvetica, but a
pack face (`/Palatino-Italic`) raised `invalidfont`, and the psart starter sketch used
exactly that face, so the first thing an agent tried exited 1.

**Behaviour (#728).** A name from a pack that is not installed no longer raises. `findfont`
answers the closest built-in face and prints one stderr line naming the pack and
`pscat fonts install --pack <pack>`; the run exits 0. Standard followed: Ghostscript
substitutes through its Fontmap and warns. Table (`font::stand_in_for`), weight and slant
kept: Palatino/Bookman/NewCenturySchlbk to Times; AvantGarde and Helvetica-Narrow to
Helvetica; ZapfChancery to Times-Italic (it has one face); Courier Prime and Special Elite
to Courier (a proportional stand-in breaks column layouts); every other `extras` face and
the `cjk-thai` faces to Helvetica, as an unknown name already is. Symbol and ZapfDingbats
also go to Helvetica, the only choice with no built-in that has the glyphs: the same codes
draw Latin letters. #599's comment gave exactly that as the reason to raise instead; the
render now goes on and the note says "its glyphs differ". Installed packs and
`fonts/catalog/` overrides are untouched.

**Why not keep `invalidfont` for Symbol/ZapfDingbats.** An agent that sets a label in
Symbol would rather get a render with a visible warning than none; the triage of #728 was
"the render must exit 0" for every pack face, so no face is special-cased to an error.

**Libraries.** `lettering`, `headline` and `jersey` verify `/Font` against the resolved
`FontName`. They only rejected Helvetica and Courier mismatches, because pack faces used to
raise before the check; a Times stand-in would have passed silently. The check now rejects
any mismatch whose resolved name begins Helvetica, Times or Courier. The `@param` text no
longer says "or invalidfont".

**Once per name.** The note is deduplicated per `Interp` (`font_notes_shown`), not per
process: a `save`/`restore` loop journals the FontDirectory entry away, and the MCP server
is long-lived. The `error_hint` mechanism #599 added for the `invalidfont` second line had no
user left and was removed.

**The CI guard (#729's suggestion).** `tests/skill_blocks.rs` extracts every fence whose
info string starts `postscript` from `plugin/skills/**/*.md` and runs it through the real
binary with `PSCAT_HOME`/`HOME`/`XDG_DATA_HOME` empty. It fails on a non-zero exit, a
`%%[ Error`, or the stand-in note: after #728 a pack face exits 0, so without the note check
the #729 mistake would pass. A sentinel test asserts `/Palatino-Roman` really stands in
there, so a leaky environment cannot make the guard vacuous. A fragment is fenced
`postscript partial` to skip it; a block with no language is never run. Debug-assertion
builds (tests) always find the checkout's `lib/` and `fonts/catalog/` through
`CARGO_MANIFEST_DIR`, so `src/paths.rs` gained `PSCAT_NO_CHECKOUT=1`, honoured only in those
builds (release builds have no heuristics to disable). It is a cargo test, so no
`.github/workflows/` change. Fixed along the way: the psart starter sketch (`/Times-Italic`),
one `paint.md` fragment that lacked its `(lib/paintkit.ps) run`, and the psviz templates were
untagged fences, now `postscript`.

**Not done (follow-ups for the owner).**
- `scripts/package_release.sh` (an owner path) writes the bundle README line "A program that
  asks for a face from a pack that is not installed stops with an error naming the pack";
  it is now false. One-line wording fix, owner merge.
- Plugin users reach pscat over MCP, where the stderr note may never reach the agent.
  Surfacing it in the tool result is `mcp_server.rs` work (#711's lane).
- The skills now say Symbol/ZapfDingbats need `--pack extras`, not texgyre as #727 guessed:
  `fonts/packs.json` puts the URW faces in `extras`.
