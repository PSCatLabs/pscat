# pscat

A PostScript interpreter compiled to WebAssembly. Render PostScript to
pixels in a browser or in Node, all at once or step by step so you can
watch the program draw. It is the engine behind the
[pscat playground](https://pscatlabs.github.io/pscat/playground.html), and
the same interpreter as the [`pscat` crate and command-line tool](https://github.com/PSCatLabs/pscat).

```sh
npm install pscat
```

Node 22 or later, or any current browser. It is ESM only, ships
TypeScript types, and has no dependencies and no install scripts.

## Render a page

```js
import { createRenderer } from 'pscat';

const r = await createRenderer();
const page = r.render(`
  0.2 0.4 0.9 setrgbcolor 72 72 288 288 rectfill
  /Helvetica-Bold findfont 36 scalefont setfont
  1 setgray 100 200 moveto (Hello) show
`, { width: 432, height: 432, dpi: 144 });

page.width;   // 864 (432 points at 144 dpi)
page.data;    // Uint8ClampedArray, RGBA, row by row from the top (the ImageData layout)
page.output;  // what the program printed with = == print stack pstack
```

In a browser, `r.paintTo(canvas)` draws the page into a `<canvas>`
(resizing it), and `r.imageData()` returns an `ImageData`.

## Watch it draw

```js
for (const _ of r.frames(source, { width: 612, height: 792, budget: 300 })) {
  r.paintTo(canvas);                        // the page so far
  await new Promise(requestAnimationFrame); // one frame per step
}
```

`frames()` runs `budget` PostScript objects per iteration. To drive it
yourself, call `r.begin(source, options)` and then `r.step(budget)` until
it returns `false`.

## Errors

```js
import { PostScriptError } from 'pscat';

try {
  r.render('1 2 bogus');
} catch (e) {
  if (e instanceof PostScriptError) {
    e.errorName; // 'undefined'
    e.command;   // 'bogus'
    e.line;      // 1
    e.report;    // '%%[ Error: undefined; OffendingCommand: bogus; Line: 1 ]%%'
    e.output;    // what the program printed before the error
  }
}
```

The renderer stays usable after an error, and the page drawn up to the
error can still be read.

## Libraries and fonts

The pscat PostScript libraries are built in, so
`(lib/artkit.ps) run`, `(lib/graph.ps) run` and the rest work with no
network requests. The [library guides](https://github.com/PSCatLabs/pscat/tree/main/docs/guides)
describe them.

The only built-in fonts are the Liberation faces behind the standard base
fonts (Helvetica, Times, Courier and their bold and italic styles). Any
other font is loaded with `addFont`, named by its font pack file stem:

```js
const bytes = await (await fetch('/fonts/EBGaramond-Regular.ttf')).arrayBuffer();
r.addFont('EBGaramond-Regular', bytes);
r.render('/Garamond findfont 24 scalefont setfont 72 72 moveto (Garamond) show');
```

The aliases work as they do with installed packs (`/Garamond`,
`/Palatino-Roman` and so on), and so does the face's own PostScript name.
The Korean, Japanese and Thai faces (`NotoSansKR-Regular`,
`NotoSansJP-Regular`, `NotoSansThai-Regular`, `NanumBrushScript-Regular`)
read their strings as UTF-8 when registered under those names. The font
files come from the `fonts-*` pack archives on the
[releases page](https://github.com/PSCatLabs/pscat/releases), each with its
own licence. Fonts are registered per renderer.

## API

| | |
|---|---|
| `createRenderer({ wasm }?)` | Load the interpreter and return a `Renderer`. Each one is a separate interpreter. `wasm` overrides where the module comes from: a URL, a `Response`, bytes or a `WebAssembly.Module` (in Node, also a file path). By default it is the package's `pscat.wasm`, which bundlers pick up through `new URL('./pscat.wasm', import.meta.url)`. |
| `render(source, { width, height, dpi }?)` | Run a program to completion on a fresh page and return `{ width, height, data, output }`. `source` is a string or a `Uint8Array`. The page is in points (default 612x792, US Letter) at `dpi` (default 72, one pixel per point). |
| `begin(source, options?)` | Queue a program on a fresh page. |
| `step(budget = 5000)` | Run up to `budget` objects. Returns `true` while there is more to do. |
| `frames(source, { budget, ...page }?)` | A generator of `begin` plus one `step` per iteration. |
| `width`, `height` | The page size in pixels. |
| `output` | What the program has printed since `begin`. |
| `pixels()` | A copy of the page as RGBA bytes. |
| `image()` | `{ width, height, data, output }` for the page as it is now. |
| `imageData()` | The page as an `ImageData` (browsers and workers). |
| `paintTo(canvas)` | Draw the page into a canvas or `OffscreenCanvas`. |
| `addFont(name, bytes)` | Make a TrueType or OpenType font available to `findfont`. |
| `PostScriptError` | Thrown by `render` and `step`. It has `errorName`, `command`, `line`, `report` and `output`. |

There is no file system and no clock: `usertime` and `realtime` read 0.
The [browser guide](https://github.com/PSCatLabs/pscat/blob/main/docs/guides/browser.md)
has more.

## Licence

`(MIT OR Apache-2.0) AND OFL-1.1`. The code is MIT or Apache-2.0, at your
option (`LICENSE-MIT`, `LICENSE-APACHE`). The SIL Open Font License 1.1
(`LICENSE-OFL`) covers the font data compiled into `pscat.wasm`: the
Liberation fonts (`fonts/LICENSE`) and the typewriter outlines generated
from Courier Prime. `THIRD_PARTY_NOTICES.md` lists every component, and
`THIRD_PARTY_LICENSES.html` holds the licences of the Rust crates compiled
into the module.

The package's version matches the `pscat` crate's.
