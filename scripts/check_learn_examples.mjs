// check_learn_examples.mjs - run every Learn example in the browser build (#252).
//
//   node scripts/check_learn_examples.mjs [EXAMPLES_DIR]   (default site/learn)
//
// The "Try it" links open these programs in the playground, which runs
// web/pscat.wasm through web/pscat.js. This drives the same two files under
// node and checks what a reader would see: no error, the printed output equal
// to NAME.out (nothing printed when there is no .out), and, for an example
// that draws (it has a %%BoundingBox), a page that is not blank. A font the
// wasm build lacks would leave the text examples blank, and this catches it.
// build_site.sh runs it after building the wasm module, so the site build in
// CI (site-check.yml, pages.yml) fails when an example breaks in the browser.
// tests/learn.rs checks the same files in the command-line pscat.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dir = process.argv[2] || join(root, 'site', 'learn');
const { Pscat } = await import(pathToFileURL(join(root, 'web', 'pscat.js')).href);
const ps = await Pscat.instantiate(readFileSync(join(root, 'web', 'pscat.wasm')));

let failed = 0;
const names = readdirSync(dir).filter((f) => f.endsWith('.ps')).sort();
for (const file of names) {
  const source = readFileSync(join(dir, file), 'utf8');
  const bbox = source.match(/^%%BoundingBox:\s*0\s+0\s+(\d+)\s+(\d+)\s*$/m);
  const [w, h] = bbox ? [Number(bbox[1]), Number(bbox[2])] : [612, 792];
  const problems = [];
  if (ps.run(source, w, h) !== 0) problems.push(`error: ${ps.error.trim()}`);
  const outFile = join(dir, file.replace(/\.ps$/, '.out'));
  const want = existsSync(outFile) ? readFileSync(outFile, 'utf8') : '';
  if (ps.output !== want) {
    problems.push(`printed:\n${ps.output}\nwanted:\n${want}`);
  }
  if (bbox) {
    const px = ps.pixels();
    let inked = 0;
    for (let i = 0; i < px.length; i += 4) {
      if (px[i] < 230 || px[i + 1] < 230 || px[i + 2] < 230) inked++;
    }
    if (inked < 50) problems.push(`blank page (${inked} inked pixels)`);
  }
  if (problems.length) {
    failed++;
    console.error(`FAIL ${file}: ${problems.join('; ')}`);
  }
}
console.log(`learn examples in the browser build: ${names.length - failed}/${names.length} ok`);
process.exit(failed ? 1 : 0);
