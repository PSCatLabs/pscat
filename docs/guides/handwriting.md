# Handwriting

pscat can write a note, card or piece of lettering in a natural, hand-drawn
script. Its handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's. Two pieces make it work:
the `handwrite` command, which turns a string into a PNG, and `/HandScript`,
the dynamic font underneath it. Both are pure PostScript, so the same file runs
in Ghostscript too.

## Design intent

The hand is meant to look human-written: legible, but loose, uneven and a
little sloppy. Regular, geometric glyphs defeat the purpose. A glyph is a bug
only when it reads as a different character, as the 4 read as a lightning bolt
(ϟ, #351) and the 7 read as ")" (#298, #410). Fix a misreading with the
smallest change that removes it, and keep the curl, wobble and asymmetry.
Compare variants at board size and pick the most human one that still reads
correctly, not the most regular one. Don't file "this glyph looks a little
odd" tidy-ups: #416 (a flat base for the 2) was closed as won't-fix for this
reason.

## The handwrite tool

```sh
pscat handwrite "meet me at the bandshell at nine"
pscat handwrite --size 44 --paper ruled --ink 0.5,0.1,0.1 \
    --seed 7 -o note.png "buy milk
call the bank about the thing"
```

Renders any string in the `/HandScript` dynamic font,
word-wrapped across lines the way a person fills a page — every
glyph is generated fresh, so repeated letters never match. The page
height is auto-sized to the text; newlines force line breaks, blank
lines are kept, and input is lowercased (the font has no capitals).

Options: `--size`, `--width`, `--height`, `--margin`, `--leading`
(line spacing × size), `--jitter` (0 = a calm hand), `--pen`,
`--ink R,G,B`, `--paper plain|ruled`, `--seed` (same seed, same
page), `--dpi`, `--halftone`, `--transparent`, `--max-pixels N` (refuse a
page over N device pixels or 8000 a side before drawing), `-o out.png`. Run with `--help` for
the full list.

The business logic lives in `lib/handscript.ps` — the font plus a
dict-driven layout API (`hs-write` draws, `hs-linecount` measures;
the script auto-sizes pages by running the count headlessly first).
The library draws nothing on load and runs unchanged in
Ghostscript, so other applications can embed the file wholesale;
the options-dict schema is documented in its header.

## The /HandScript font

`examples/handwriting.ps` shows the font on its own. `/HandScript` is a Type 3
font whose glyphs are generated fresh on every draw, with random-jittered
strokes, wandering baselines and varying pen pressure. No two letters ever
match, and every page is still reproducible, because the jitter comes from a
seed. The gallery piece *Hundred Lines* uses it to write punishment lines on a
chalkboard, with the jitter climbing line by line.

The `pscat-mcp` server exposes the same tool, so an agent can ask for a
handwriting-style note directly (see the [agents guide](agents.md)).
