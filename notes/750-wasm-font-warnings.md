# wasm/npm: say when a pack font is substituted (#750)

Native builds print a stderr note when a font from an uninstalled pack
stands in for a built-in face (#728). The wasm build has no stderr and no
packs, so `/Bangers findfont` silently became Helvetica.

- `font::resolve` now runs the pack check on wasm too, after the faces
  registered with `addFont` and the embedded standard 35, so an added face
  never warns. The note is worded for a browser (`addFont('Bangers-Regular',
  bytes)`, not `pscat fonts install`); the stem is the one that matched
  (`/Bangers` finds the pack through `Bangers-Regular`).
- `findfont` appends each note, once per name, to `Interp::font_warnings`
  (newline-joined). Native still prints it to stderr, unchanged.
- `ps_warnings_ptr`/`ps_warnings_len` expose it; `web/pscat.js` has
  `warnings`, and the npm `Renderer.warnings` plus `image()`/`render()`'s
  `warnings` array carry it. Per run, since each `begin` builds a fresh
  interpreter; readable after a `PostScriptError`.
- Not changed: names no pack lists still fall back to Helvetica silently,
  as on native.
