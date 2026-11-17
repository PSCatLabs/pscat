<!-- cloud: replace-file -->
# Handwriting-style notes and LaTeX on a chalkboard

Read this for notes, cards and lettering in a hand-drawn script, or for a
formula on a chalkboard. Back to [SKILL.md](../SKILL.md).

## Handwriting-style notes (text → PNG): `handwrite`

pscat's handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's. If a request asks for a named or real person's
hand, or for a document that passes as written by someone, decline that part
and offer a generic hand instead (the default script, another `seed`).

Call `handwrite` with `text` (newlines force line breaks). Optional: `width`
(the page in points, default 612), `size` (the font size in points, default 36),
`paper` (`plain` or `ruled`), `ink` (a string `"R,G,B"`, each from 0 to 1),
`jitter` (a whole number from 0 to 10000, default 16: 0 is calm), `seed`
(the same seed gives the same note) and `dpi`. It word-wraps to `width` and sets
the page height to fit the text, and returns a PNG.

The script is a natural, hand-drawn one: jittered strokes, no two letters alike.
It has the letters a to z (**capitals are written as lowercase**), the digits 0
to 9 and `. , - ' ! ?`; any other character leaves a gap. From PostScript, load
it with `(lib/handscript.ps) run` (one options dict; see its header, and ask
`describe_art_capabilities` for the entry).

## LaTeX on a chalkboard (formula → PNG, SVG or PDF): `chalkboard_math`

Call `chalkboard_math` with `latex`: raw LaTeX with **single** backslashes
(unlike mathkit from PostScript; in the tool call's JSON each backslash is
doubled, `"x = \\frac{1}{2}"`), `\\` between rows, `&` to align; `$...$` or an
`align` wrapper is fine. The block is shrunk to fit the board and centred, and
the same arguments give the same image.

Optional arguments: `format` (`png`, `svg` or `pdf`), `seed`, `size` (the writing
size in points, default 64), `color` (`white`, `yellow`, `pink`, `blue`, `green`
or `orange`), `board` (`true` for a slate, `false` for chalk alone on a
transparent page, `"slate"`, `"greenboard"`, an `[r, g, b]` colour or an object of
board keys), `align` (`center` or `left`), `ghosting` (how much erased earlier
writing shows, 0 to 1, or `false`), `irregular` (0 clean and level, up to 5),
`jitter`, `slope`, `width` and `height` (the board in points, default 800 by
400), `dpi` (default 144) and `config` for the rest (groups `layout`, `chalk`
and `board`, and `seed`).

TeX outside mathkit's subset fails with an error that names the command. SVG and PDF keep every chalk grain as a
vector mark, so a board is megabytes as SVG: prefer PNG unless a vector is
needed. For a composed piece (several formulas, words, annotations, a board of
your own) use mathkit from PostScript, which the `psart` skill's board-math
reference describes.
