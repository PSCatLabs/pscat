# pscat extensions and smaller tools

Read this for pscat-only operators (translucency, native noise,
`writereals`/`readreals`), photo-to-engraving, the live window, or the
capabilities inventory. Back to [SKILL.md](../SKILL.md).

## Translucency: setalpha / setblendmode (pscat extensions)

```postscript
0.3 setalpha              % fill/stroke opacity, 0..1
/Multiply setblendmode    % or /Normal (the default); nothing else
0.8 0.3 0.2 setrgbcolor 72 72 200 200 rectfill   % draws at that alpha and blend mode
currentalpha currentblendmode pop pop              % read them back
```

Graphics state (`gsave`/`grestore` save them, `initgraphics` resets
them). They reach fills, strokes, text, `shfill` and, since #766,
`image`/`imagemask`/`colorimage` (an image at alpha 0.5 is half
see-through; `Multiply` darkens what is under it; a soft-masked
ImageType 3 multiplies its own mask by the alpha). Exported to SVG and
PDF as well as PNG. **Not PLRM
operators**: Ghostscript has no PostScript-level alpha, so hand someone a
`--pdf` or `--png`, not a `.ps`, and probe with `systemdict /setalpha
known` in code that must run in both. Also pscat-only: `gstate .sameclip
bool` (is the clip in force the one the gstate captured).

## Noise: gnoise2 / fbm2 / ... (pscat extensions)

```postscript partial
42 noiseseed              % own table; never touches rand (default: 0)
x y gnoise2 n             % gradient noise ~[-0.71,0.71]; x y z gnoise3 ~[-1,1]
x y vnoise2 v             % value noise ~[-0.996,0.996]; x y z vnoise3
x y gnoise2d n dx dy      % with its analytic gradient
x y 5 0.5 2 fbm2 v        % oct gain lacunarity; fbm3 vfbm2 vfbm3 ridged2 ridged3
```

Native, about as cheap as an empty loop step, and bit-identical on every
platform. `lib/noiseref.ps` (`nrgnoise2` ...) is the same algorithm in
PostScript for Ghostscript-bound code; probe with `systemdict /gnoise2
known`. artkit's `noise2` already runs natively. For a heavy per-pixel noise
render, see `--bands` in [bands-profile.md](bands-profile.md).

## Save and reload a big numeric array: writereals / readreals

To save or reload a big numeric array, use `array (path) writereals` and
`(path) n readreals` (packed binary64; add `/f32` before the operator for
binary32), not printing and `run`ning text: exact, and about 10x faster to
read. Writing needs `--allow-write DIR` like `(w) file`. `(path) /all
readreals` reads every value, `(path) 100 50 readreals` is 50 values from
value 100, and `array file writereals` / `file n readreals` take an open
file (`%stdout`, an `(a)` append file). With `pscat --allow-write out f.ps`:

```postscript partial
[ 0 1 99 { 0.1 mul dup 5 0.5 2 fbm2 } for ] (out/field.bin) /f32 writereals
(out/field.bin) /all /f32 readreals length =      % 100
```

## Photo → engraving

```sh
pscat photo-etch photo.jpg -o etching.png      # or .svg / .pdf
pscat photo-etch --spacing 4 --angle 30 photo.jpg -o etch.svg
```

## Watch it draw (needs a display)

```sh
pscat file.ps                          # live window; --speed N paces it
pscat -i                               # REPL + window: type, watch it draw
pscat --spool DIR                      # render every .ps that lands in DIR
```

Arrow keys browse completed pages in any window mode. `--spool` runs
whatever lands in `DIR`: only point it at a directory you trust.

## Capabilities (structured, for agents)

```sh
pscat --capabilities                   # fonts, palettes, templates, procedures, native pscat operators as JSON
```

The machine-readable inventory of what this build actually has: fonts,
Type 3 faces, artkit's palettes, `lib/pagekit.ps`'s templates, and the
library's major procedures with their parameters, plus `pscat_version`.
Prefer it over guessing a name from prose (the skill files included).
