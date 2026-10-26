# npm/playground: font warnings next to errors (#804)

Follow-up to #750 (notes/750-wasm-font-warnings.md).

- `PostScriptError` has a `warnings` array: the font substitution notes up
  to the error, as `Renderer.warnings` and `RenderResult.warnings` give
  them. `step()` (so `render()` and `frames()`) passes them in when it
  throws. It is a snapshot: a later run on the same renderer does not
  change it. The constructor's third parameter defaults to `[]`, so code
  that builds a `PostScriptError` by hand keeps working.
- `web/index.html` shows `ps.warnings` in a `#warn` element under the
  error, in an amber colour, after every frame, so a run that finishes
  cleanly but drew Bangers as Helvetica says why too. It is cleared when
  Run restarts. `web/pscat.js` needed no change.
- Not browser-tested: the playground change is five lines of DOM and was
  checked by reading, not in a browser. The npm half is covered by
  `npm/test/api.test.mjs`.
- `site/playground.html` is a separate hand-kept page and is not changed
  here (PR #794 edits it); it can take the same `#warn` element after that
  merges.
