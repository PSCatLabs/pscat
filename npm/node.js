// pscat for Node (>= 22): the index.js API with pscat.wasm read from disk, since
// Node's fetch cannot load file: URLs. The `node` condition in
// package.json's exports map selects this file.

import { readFile } from 'node:fs/promises';
import { instantiate, once, toModule } from './renderer.js';

export { PostScriptError, Renderer } from './renderer.js';

async function load(src) {
  if (typeof src === 'string' && /^file:/i.test(src)) src = new URL(src);
  if (src instanceof URL && src.protocol === 'file:') return toModule(await readFile(src));
  // A path, including a Windows drive path (a URL scheme here has two or more letters).
  if (typeof src === 'string' && !/^[a-z][a-z0-9+.-]+:/i.test(src)) return toModule(await readFile(src));
  return toModule(src);
}

const defaultModule = once(() => load(new URL('./pscat.wasm', import.meta.url)));

/**
 * Load the interpreter and return a Renderer. `options.wasm` may also be
 * a file path or a file: URL here.
 */
export async function createRenderer(options = {}) {
  if (options.wasm !== undefined) return instantiate(await load(options.wasm));
  return instantiate(await defaultModule());
}
