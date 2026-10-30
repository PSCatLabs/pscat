# Handwriting-style notes and LaTeX on a chalkboard

Read this for notes, cards and lettering in a hand-drawn script, or for a
formula on a chalkboard. Back to [SKILL.md](../SKILL.md).

## Handwriting-style notes (string → PNG)

pscat's handwriting is a set of hand-drawn styles for notes, cards and art.
It isn't meant for imitating a specific person's handwriting or presenting
writing as someone else's. If a request asks for a named or real person's
hand, or for a document that passes as written by someone, decline that part
and offer a generic hand instead (the default /HandScript, another `--seed`).

```sh
pscat handwrite "meet me at the bandshell at nine" -o note.png
pscat handwrite --paper ruled --size 30 --seed 7 "text"
pscat handwrite --ink 0.4,0.1,0.1 --jitter 24 "shakier, redder"
```

The /HandScript dynamic font, a natural, hand-drawn script: jittered strokes, no two letters alike,
reproducible per `--seed`. Word-wraps and auto-sizes the height.
**Lowercase only**: input is lowercased, and characters outside a–z 0–9
`.,-'!?` advance invisibly. `pscat handwrite --help` lists every option.
From PostScript: `(lib/handscript.ps) run` (one options dict; see its
header). Over MCP: the `handwrite` tool.

## LaTeX on a chalkboard (formula → PNG/SVG/PDF)

```sh
pscat chalk 'x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}' -o board.png
pscat chalk 'f(x) &= (x+1)^2 \\ &= x^2 + 2x + 1' --seed 7 --color yellow -o derivation.png
pscat chalk --file eq.tex --json '{"board": {"tone": "greenboard"}}' -o eq.png
pscat chalk 'e^{i\pi} + 1 = 0' --no-board -o overlay.png    # transparent, chalk only
```

Raw LaTeX with **single** backslashes (unlike mathkit from PostScript),
`\\` between rows, `&` to align; `$...$` or an `align` wrapper is fine.
Deterministic per `--seed`. The block is shrunk to fit the board and centred.
TeX outside mathkit's subset fails with one line naming it
(`unsupported LaTeX command \aleph`) and writes nothing. `--config FILE` /
`--json TEXT` take the JSON config (`layout`, `chalk`, `board` or
`"board": false`, `seed`); `pscat chalk --help` and the dry-media guide list
every key. `--print-ps` prints the generated PostScript.

With the MCP tools connected, use **`chalkboard_math`** for a chalkboard or
an equation on a board: `{"latex": "x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}", "seed": 7}`
(JSON escaping only; the LaTeX itself has single backslashes) returns the
PNG inline. The same keys as flat arguments (`size`, `color`, `board` or
`false`, `ghosting`, `width`/`height`, `dpi`, ...), `config` for the rest; a
TeX error is a one-line `latex_error` naming the command.
