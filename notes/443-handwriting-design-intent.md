## Handwriting design intent stated where agents read it (issue #443, 2026-10-14)

PR #441 (#416, a flat base for handscript's 2) was closed on review: the flat
base was slightly more readable but looked less like a human wrote it. The
maintainer wants legible handwriting that is a little sloppy, because that is
more realistic. #416 came from an agent that read a loose glyph as a defect;
nothing in the repo said the looseness was deliberate.

Added a short "Design intent" statement, same four points each time, in
`docs/guides/handwriting.md` (new `## Design intent` section, the full text),
and in the header comments of `lib/handscript.ps` and `lib/strokeglyphs.ps`
(shorter, linking to the guide anchor):

1. The hand should look human: legible, loose, uneven, a little sloppy.
2. A glyph is a bug only when it reads as a different character (4 as a
   lightning bolt, #351; 7 as ")", #298/#410).
3. Fix with the smallest change; keep curl, wobble and asymmetry; compare at
   board size and choose the most human variant that reads correctly.
4. Don't file "looks a little odd" tidy-ups (#416 was won't-fix).

strokeglyphs' comment says its 4 is an override but its 7 is handscript's own
(#298's doubled corner), since it only overrides the 4.

Comment-only change in both `.ps` files, no `% @` lines added (build.rs reads
those). Verified with `scripts/identity_check.sh --loader` for each library,
plus a marker control. Not done: a line in `.claude/skills/psart/` or
`AGENTS.md`; neither holds glyph-shape conventions (psart only says how to load
the libraries), and the issue made it conditional on that.
