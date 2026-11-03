# QR codes

`lib/qr.ps` encodes text as a QR Model 2 symbol and draws it as vector
geometry. It is ordinary PostScript with no dependencies. It is compiled
into `pscat`, so `(lib/qr.ps) run` works anywhere, with no checkout. It
also runs in Ghostscript.

```postscript
(lib/qr.ps) run
72 72 moveto
(https://github.com/PSCatLabs/pscat) << >> qrencode
<< /ModuleSize 4 >> qrdraw
showpage
```

That draws a 148-point square (29 modules plus a 4-module quiet zone on
each side, 4 points per module) with its lower-left corner at (72, 72).
`pscat --png`, `--svg` and `--pdf` export the same geometry.

## Encode: `payload opts qrencode -> symbol`

`payload` is a string of raw bytes. Plain ASCII, such as a URL, works as
it is. pscat does no Unicode conversion and writes no ECI header, so for
non-ASCII text, pass UTF-8 bytes and expect most scanners to guess the
encoding. Every symbol uses byte mode.

| Option | Value | Default |
|---|---|---|
| `/Level` | Error correction `/L`, `/M`, `/Q` or `/H` (about 7, 15, 25 and 30 % of the symbol recoverable). Used exactly as given, never raised. | `/M` |
| `/Version` | A fixed version 1–40 (side `17 + 4 * version` modules). `null` picks the smallest version that fits. | `null` |
| `/Mask` | A forced data mask 0–7. `null` scores all eight masks with the standard's penalty rules and keeps the lowest, which is what you want unless you are matching a reference. | `null` |

The result is a new dictionary:

| Key | Meaning |
|---|---|
| `/Version` | 1–40. |
| `/Level` | The level used. |
| `/Mask` | The mask used. |
| `/Size` | Modules per side, `17 + 4 * Version`, quiet zone not included. |
| `/Modules` | An array of `Size` rows, the **top row first**. Each row is an array of `Size` booleans, the **leftmost module first**. `true` is dark. |

So `sym /Modules get y get x get` is the module in column `x`, row `y`,
counted from the top-left corner as the QR standard counts them. The
arrays are new on every call and belong to you. Change them if you want:
`qrdraw` draws whatever they hold. A bad symbol can stop scanning, though.

```postscript
(lib/qr.ps) run
/sym (https://github.com/PSCatLabs/pscat) << /Level /H >> qrencode def
sym /Version get =                          % 4: H needs more room than M
sym /Modules get 0 get 0 get =              % true: the top-left finder
```

## Draw: `symbol opts qrdraw -`

`qrdraw` draws the symbol with the **lower-left corner of its quiet zone
at the current point**. Use `x y moveto` first. A missing current point
raises `qr-draw-needs-a-current-point`. The current transformation applies,
so a translated, rotated or scaled coordinate system moves the symbol with
it.

| Option | Value | Default |
|---|---|---|
| `/ModuleSize` | The side of one module, in user units (points). | none |
| `/Side` | The side of the whole drawing, quiet zone included, in user units. The module size is `Side / (Size + 2 * QuietZone)`. | none |
| `/QuietZone` | The light margin in modules on every side. An integer, at least 4, the minimum the standard requires. | `4` |
| `/Dark` | The dark-module color: a gray level, or `[gray]`, `[r g b]` or `[c m y k]`, each component 0–1. | `0` (black) |
| `/Light` | The light-module and quiet-zone color, in the same forms. `null` paints nothing, so the surface below shows through. | `1` (white) |

Give exactly one of `/ModuleSize` and `/Side`. Use `/Side` to fill a fixed
space, for example a 0.8-inch square on a business card:

```postscript
(lib/qr.ps) run
20 20 moveto
(https://github.com/PSCatLabs/pscat) << >> qrencode
<< /Side 57.6 /Dark [0.1 0.15 0.35] /Light [1 0.97 0.9] >> qrdraw
showpage
```

What `qrdraw` paints:

1. A light square (unless `/Light` is `null`).
2. One filled path holding one rectangle per horizontal run of dark modules.

Every corner is `origin + k * module` for a whole number `k`, so
neighboring modules share their edges exactly. Because the runs are one
path filled once, no hairline gaps appear between modules at any scale or
resolution, in PNG, SVG or PDF. For the crispest PNG, choose a module size
that is a whole number of device pixels (`--dpi 72` makes 1 point 1 pixel)
and an origin on the pixel grid.

`qrdraw` leaves your graphics state (color, line width, transformation,
clip), your current path and current point, and the dictionary stack as
they were. It takes its two operands and leaves nothing.

## Scanning tips

- Keep the quiet zone light. If the symbol sits on a dark or busy
  background, leave `/Light` at a light color so the quiet zone is painted.
- Dark modules must be darker than light ones. `qrdraw` refuses the
  reverse (`qr-dark-must-be-darker-than-light`), because many scanners
  cannot read an inverted symbol. It does not check contrast beyond that:
  navy on cream scans well, mid-gray on light gray may not.
- For print, a common rule of thumb is at least about 0.3 mm (0.85 pt) per
  module for close-range phone scanning. Larger is safer. `/Level /Q` or `/H` buys tolerance for
  wear and smudging at the cost of a larger version.

## Errors

Every argument is checked before anything is drawn, so an error leaves the
page, the graphics state and the dictionary stack untouched. Errors are
named `undefined` errors, like the other pscat libraries:

| Error | Cause |
|---|---|
| `qr-options-must-be-a-dict` | The options operand is not a dictionary. |
| `qr-data-must-be-a-string`, `qr-data-must-be-readable` | Bad payload. |
| `qr-level-must-be-L-M-Q-or-H` | `/Level` is not one of the four names. |
| `qr-version-must-be-integer`, `qr-version-must-be-in-1-40` | Bad `/Version`. |
| `qr-mask-must-be-integer`, `qr-mask-must-be-in-0-7` | Bad `/Mask`. |
| `qr-data-too-long-for-level` | The payload does not fit even version 40 at that level (2953, 2331, 1663 and 1273 bytes at L, M, Q and H). |
| `qr-data-too-long-for-version` | The payload does not fit the fixed `/Version`. |
| `qr-symbol-must-be-a-qr-symbol` | `qrdraw` was not given a symbol with a valid `/Size` and `Size` rows of `Size` booleans in `/Modules`. |
| `qr-draw-needs-modulesize-or-side`, `qr-draw-takes-modulesize-or-side-not-both` | Neither size option, or both. |
| `qr-module-size-must-be-a-positive-number`, `qr-side-must-be-a-positive-number` | Bad size. |
| `qr-quiet-zone-must-be-an-integer`, `qr-quiet-zone-must-be-at-least-4` | Bad `/QuietZone`. |
| `qr-dark-must-be-a-color`, `qr-light-must-be-a-color-or-null` | Bad color. |
| `qr-dark-must-be-darker-than-light` | `/Dark` is as light as `/Light` or lighter. |
| `qr-draw-needs-a-current-point` | No `moveto` before `qrdraw`. |

After an error, operands may remain on the stack.

## How it is checked

The encoder is checked against frozen output from an independent encoder
(Nayuki's QR-Code-generator) at every stage, and Ghostscript reproduces it.
The test suite renders symbols through `pscat --png`, `--svg` and `--pdf`.
It rasterizes the SVG with resvg and the PDF with hayro, two renderers
that share no code with pscat, and checks every module and the quiet zone.
Then the independent `rqrr` decoder must read back the exact text, version,
mask and level. The details are in
[`docs/dev/design/QR_CODE_TESTING.md`](../dev/design/QR_CODE_TESTING.md).
