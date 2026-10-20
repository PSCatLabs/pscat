// pscat: a PostScript interpreter compiled to WebAssembly (#566).
//
// The browser and bundler entry point. node.js is the Node one; both
// share renderer.js. The API is documented in README.md and typed in
// index.d.ts.

import { instantiate, once, toModule } from './renderer.js';

export { PostScriptError, Renderer } from './renderer.js';

const defaultModule = once(() => toModule(new URL('./pscat.wasm', import.meta.url)));

/**
 * Load the interpreter and return a Renderer. By default the module is
 * pscat.wasm next to this file (bundlers resolve the URL); pass
 * `options.wasm` to load it from elsewhere.
 */
export async function createRenderer(options = {}) {
  if (options.wasm !== undefined) return instantiate(await toModule(options.wasm));
  return instantiate(await defaultModule());
}
