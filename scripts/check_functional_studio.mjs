// Execute the exact offline studio payload with the real PSCat WASM/API.
// Node standard library only; optional --out-dir writes RGBA for native parity.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
import {Pscat} from '../web/pscat.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const runtime = await fs.readFile(path.join(root, 'docs/functional-postscript/assets/pscat-studio-runtime.js'), 'utf8');
const context = {window: {}};
vm.runInNewContext(runtime, context);
const bundle = context.window.PSCAT_STUDIO_BUNDLE;
const ps = await Pscat.instantiate(gunzipSync(Buffer.from(bundle.wasmGzipBase64, 'base64')));
const arg = process.argv.indexOf('--out-dir');
const outDir = arg < 0 ? null : process.argv[arg + 1];
if (outDir) await fs.mkdir(outDir, {recursive:true});
const cases = [
  {name:'terrain', mode:'terrain', width:960, height:1200, setup:'',
    output:'enter type-terrain\nleave type-terrain\nglyphs=15\n'},
  {name:'press', mode:'press', width:1280, height:960, setup:'',
    output:'enter proof\nleave proof\nattempted=7\ncompleted=6\nrejected=1\nfault=obs-font-size-must-be-number\n'},
  {name:'press-valid', mode:'press', width:1280, height:960,
    setup:'/Inspect false def /InjectFault false def /ProofText (DREAM) def\n',
    output:'enter proof\nleave proof\nattempted=5\ncompleted=5\nrejected=0\nfault=none\n'}
];
for (const test of cases) {
  assert.equal(ps.run(test.setup + bundle.programs[test.mode], test.width, test.height), 0, ps.error);
  assert.equal(ps.output, test.output, test.name);
  assert.equal(ps.warnings.length, 0, test.name + ': font stand-in');
  assert.equal(ps.pixels().length, test.width * test.height * 4);
  if (outDir) await fs.writeFile(path.join(outDir, test.name + '.rgba'), ps.pixels());
  console.log(test.name + ': WASM executed; actual stdout and dimensions match');
}
