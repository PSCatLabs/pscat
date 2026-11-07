#!/usr/bin/env node
// build_npm_global.mjs: make npm/pscat.global.js (#1025), the package's
// classic-script build, from the same two sources the ES modules use:
// web/pscat.js (the low-level wrapper, shipped as core.js) and
// npm/renderer.js (Renderer, PostScriptError and the module loaders).
//
//   node scripts/build_npm_global.mjs            # writes npm/pscat.global.js
//   node scripts/build_npm_global.mjs OUT        # writes OUT instead
//
// The result is one script with no import, export or import.meta, so it
// runs from a plain <script> tag (including from file://), as a classic
// worker's importScripts() target, or pasted inline into a page. It defines
// one global, `pscat`: { createRenderer, PostScriptError, Renderer },
// the same API as the ES module's. Nothing needs a bundler, and the only
// thing it cannot do by itself is find pscat.wasm when it cannot tell where
// it was loaded from (see wasmBase below): pass createRenderer({ wasm }).
//
// build_npm.sh runs this after it copies the wasm; the transformation is
// deliberately plain text surgery with a loud failure for each assumption,
// and the generated file is parsed as a classic script (vm.Script) before
// it is written, so a source change that brings module syntax in fails the
// build here rather than in a user's browser.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const out = process.argv[2] ?? join(root, 'npm', 'pscat.global.js');

function fail(msg) {
  console.error(`build_npm_global.mjs: ${msg}`);
  process.exit(1);
}

// Replace `from` (a string that must occur exactly once) with `to`.
function replaceOnce(text, from, to, what) {
  const n = text.split(from).length - 1;
  if (n !== 1) fail(`${what}: expected one occurrence of ${JSON.stringify(from)}, found ${n}`);
  return text.replace(from, () => to);
}

// Drop the `export ` that starts a declaration line; every such declaration
// must be one of `names`, so a new export is noticed.
function stripExports(text, file, names) {
  const seen = [];
  const body = text.replace(/^export (?:default )?(class|async function|function|const|let) (\w+)/gm, (_, kind, name) => {
    seen.push(name);
    return `${kind} ${name}`;
  });
  const extra = seen.filter((n) => !names.includes(n));
  if (extra.length) fail(`${file} exports ${extra.join(', ')}, which this script does not know about`);
  const other = body.match(/^\s*export\b.*$/m);
  if (other) fail(`${file}: unhandled export syntax: ${other[0].trim()}`);
  return body;
}

const version = JSON.parse(readFileSync(join(root, 'npm', 'package.json'), 'utf8')).version;

let core = readFileSync(join(root, 'web', 'pscat.js'), 'utf8');
core = stripExports(core, 'web/pscat.js', ['Pscat']);
// Drop the file's header comment (it describes the ES module).
const header = core.slice(0, core.indexOf('\nclass Pscat') + 1);
if (!/^(\/\/.*\n|\n)*$/.test(header)) fail('web/pscat.js: expected only a // comment header before class Pscat');
core = core.slice(header.length);
// Pscat.load's default URL is import.meta.url-relative; the global build has
// its own notion of where the module is.
core = replaceOnce(core, "new URL('./pscat.wasm', import.meta.url)", 'wasmUrl()', 'web/pscat.js default URL');

let renderer = readFileSync(join(root, 'npm', 'renderer.js'), 'utf8');
renderer = replaceOnce(renderer, "import { Pscat } from './core.js';\n", '', 'renderer.js import');
renderer = stripExports(renderer, 'npm/renderer.js', ['PostScriptError', 'Renderer', 'toModule', 'instantiate', 'once']);

const entry = `
// --- the classic-script entry (what index.js is to the ES modules) ---------

// Where this script was loaded from, read NOW: document.currentScript is
// null by the time an async function runs. It is empty for an inline
// <script> (nothing to be relative to), for a blob: or data: URL, and in a
// worker (self.location is the worker's own script, not necessarily this
// one), and then pscat.wasm has to be named with createRenderer({ wasm }).
const wasmBase = (() => {
  try {
    const src = typeof document !== 'undefined' && document.currentScript && document.currentScript.src;
    return src && /^(https?|file):/i.test(src) ? src : null;
  } catch (e) {
    return null;
  }
})();

function wasmUrl() {
  if (wasmBase === null) {
    throw new Error(
      'pscat: cannot tell where pscat.wasm is (this script was not loaded from a URL, e.g. it is inline ' +
        'or in a worker); pass createRenderer({ wasm }) a URL, a Response or the module bytes',
    );
  }
  return new URL('pscat.wasm', wasmBase);
}

const defaultModule = once(async () => toModule(wasmUrl()));

/**
 * Load the interpreter and return a Renderer. By default the module is
 * pscat.wasm next to this script; pass \`options.wasm\` (a URL, a Response,
 * bytes or a WebAssembly.Module) to load it from elsewhere.
 */
async function createRenderer(options = {}) {
  if (options.wasm !== undefined) return instantiate(await toModule(options.wasm));
  return instantiate(await defaultModule());
}

globalThis.pscat = { createRenderer, PostScriptError, Renderer };
`;

const text = `/* pscat ${version}: classic-script build (no modules, no bundler). Generated by
 * scripts/build_npm_global.mjs from web/pscat.js and npm/renderer.js; do not edit.
 *
 *   <script src="pscat.global.js"></script>
 *   <script>
 *     pscat.createRenderer().then((r) => { r.render('...'); r.paintTo(canvas); });
 *   </script>
 *
 * Defines one global, pscat = { createRenderer, PostScriptError, Renderer }:
 * the API of the ES module (see README.md). Finds pscat.wasm next to itself
 * unless createRenderer({ wasm }) says otherwise. (MIT OR Apache-2.0)
 */
(function () {
'use strict';

${core.trimEnd()}

${renderer.trimEnd()}
${entry}})();
`;

try {
  new vm.Script(text, { filename: 'pscat.global.js' }); // classic-script parse: module syntax is a SyntaxError
} catch (e) {
  fail(`the generated script does not parse as a classic script: ${e.message}`);
}
if (/\bimport\s*\(/.test(text.replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, ''))) fail('a dynamic import() survived');
writeFileSync(out, text);
console.log(`build_npm_global.mjs: wrote ${out} (${text.length} bytes)`);
