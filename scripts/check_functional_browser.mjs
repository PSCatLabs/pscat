// Optional real-browser verification of the offline studio; no npm dependency.
// Start Chrome with --remote-debugging-port=9338, then run this script.
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const port = process.argv[2] || '9338';
const targets = await (await fetch('http://127.0.0.1:' + port + '/json/list')).json();
const ws = new WebSocket(targets.find(t => t.type === 'page').webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let serial = 0;
const pending = new Map(), exceptions = [], requests = [];
ws.onmessage = ({data}) => {
  const m = JSON.parse(data);
  if (m.id) {
    const p = pending.get(m.id); pending.delete(m.id);
    m.error ? p.reject(m.error) : p.resolve(m.result);
  } else if (m.method === 'Runtime.exceptionThrown') exceptions.push(m.params);
  else if (m.method === 'Network.requestWillBeSent') requests.push(m.params.request.url);
};
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++serial; pending.set(id, {resolve,reject}); ws.send(JSON.stringify({id,method,params}));
  });
}
async function evaluate(expression) {
  const result = await call('Runtime.evaluate', {expression, awaitPromise:true, returnByValue:true});
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
async function done(wanted = 'complete') {
  const result = await evaluate(`new Promise(resolve=>{const start=performance.now();function poll(){const state=document.documentElement.dataset.renderState;if(state==='${wanted}'||state==='error'||performance.now()-start>30000)resolve({state,status:document.getElementById('status')?.textContent,output:document.getElementById('stdout')?.textContent});else setTimeout(poll,30)}poll()})`);
  assert.equal(result.state, wanted, JSON.stringify(result));
  return result;
}
async function pixels() {
  return evaluate(`(()=>{const c=document.getElementById('canvas'),d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;let h=2166136261;for(const v of d)h=Math.imul(h^v,16777619);return (h>>>0).toString(16)})()`);
}
async function screenshot(name, id = 'studio') {
  await evaluate(`document.getElementById('${id}').scrollIntoView({behavior:'instant'})`);
  const shot = await call('Page.captureScreenshot', {format:'png'});
  await fs.writeFile(path.join(root,'target',name+'.png'), Buffer.from(shot.data,'base64'));
  report.screenshots.push('target/'+name+'.png');
}
async function layout(width,height,mobile) {
  await call('Emulation.setDeviceMetricsOverride', {width,height,deviceScaleFactor:1,mobile});
  const size = await evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve({w:innerWidth,scroll:document.documentElement.scrollWidth}))))`);
  assert.equal(size.w,width); assert.ok(size.scroll<=width,JSON.stringify(size));
}
const report = {browser:(await call('Browser.getVersion')).product, transport:'file://',
                checks:[], screenshots:[], output:{}};
try {
  await call('Page.enable'); await call('Runtime.enable'); await call('Network.enable');
  await layout(1440,1100,false);
  await call('Page.navigate', {url:'file://'+path.join(root,'docs/functional-postscript/live-studio.html')});
  report.output.terrain = (await done()).output;
  assert.equal(report.output.terrain, 'enter type-terrain\nleave type-terrain\nglyphs=15\n');
  const original = await pixels();
  await screenshot('functional-live-type-desktop');
  await evaluate(`document.getElementById('word-a').value='CODE';document.getElementById('word-b').value='MAKES';document.getElementById('word-c').value='LIGHT';document.getElementById('render').click()`);
  report.output.edited = (await done()).output;
  assert.ok(report.output.edited.endsWith('glyphs=14\n'));
  const edited = await pixels(); assert.notEqual(edited,original);
  await evaluate(`document.getElementById('palette').value='1';document.getElementById('render').click()`);
  await done(); assert.notEqual(await pixels(),edited);
  const treatments = await evaluate(`Array.from(document.getElementById('palette').options,o=>o.value)`);
  const fonts = await evaluate(`Array.from(document.getElementById('type-font').options,o=>o.value)`);
  assert.equal(treatments.length,6); assert.equal(fonts.length,8);
  const treatmentPixels = new Set();
  for (const value of treatments) {
    await evaluate(`document.getElementById('palette').value='${value}';document.getElementById('palette').dispatchEvent(new Event('change'))`);
    await done(); treatmentPixels.add(await pixels());
  }
  assert.equal(treatmentPixels.size,6,'treatments must produce distinct actual canvases');
  const fontPixels = new Set();
  for (const font of fonts) {
    await evaluate(`document.getElementById('type-font').value='${font}';document.getElementById('type-font').dispatchEvent(new Event('change'))`);
    await done(); fontPixels.add(await pixels());
    assert.ok(await evaluate(`document.getElementById('source').value.includes('/TypeFont /${font} def')`));
  }
  assert.equal(fontPixels.size,8,'font choices must change the actual render');
  report.treatments = treatments.length; report.fonts = fonts;
  await evaluate(`document.getElementById('palette').value='4';document.getElementById('palette').dispatchEvent(new Event('change'));document.getElementById('type-font').value='Times-BoldItalic';document.getElementById('type-font').dispatchEvent(new Event('change'))`);
  await done(); await screenshot('functional-live-neon-desktop');
  await evaluate(`document.getElementById('word-a').value=') quit (';document.getElementById('render').click()`);
  await done('invalid');
  await evaluate(`document.getElementById('word-a').value='CODE';document.querySelector('[data-mode="press"]').click()`);
  report.output.press = (await done()).output;
  assert.ok(report.output.press.includes('attempted=7\ncompleted=6\nrejected=1\n'));
  const inspected = await pixels();
  await screenshot('functional-live-press-desktop');
  await evaluate(`document.getElementById('inspect').checked=false;document.getElementById('render').click()`);
  assert.equal((await done()).output,report.output.press);
  assert.notEqual(await pixels(),inspected);
  await evaluate(`document.getElementById('inject-fault').checked=false;document.getElementById('render').click()`);
  assert.ok((await done()).output.includes('attempted=6\ncompleted=6\nrejected=0\nfault=none\n'));
  await evaluate(`document.getElementById('proof-text').value='DREAM';document.getElementById('inject-fault').checked=true;document.getElementById('render').click()`);
  report.output.changedProof = (await done()).output;
  assert.ok(report.output.changedProof.includes('attempted=6\ncompleted=5\nrejected=1\n'));
  await evaluate(`document.querySelector('[data-mode="terrain"]').click()`); await done();
  await evaluate(`document.getElementById('watch').click();new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>{document.getElementById('stop').click();resolve()})))`);
  await done('stopped');
  await evaluate(`document.getElementById('render').click()`); await done();
  await evaluate(`document.querySelector('[data-mode="press"]').click()`); await done();
  const downloadDir = await fs.mkdtemp(path.join(root,'target/functional-studio-downloads-'));
  await call('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:downloadDir});
  await evaluate(`document.getElementById('download').click()`);
  // Wait for the browser's actual Blob PNG download, not a fabricated file.
  let download;
  for (let attempt=0;attempt<100;attempt++) {
    try { download=await fs.readFile(path.join(downloadDir,'observable-press.png')); break; }
    catch { await new Promise(resolve=>setTimeout(resolve,50)); }
  }
  assert.ok(download && download.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
  assert.equal(download.readUInt32BE(16),1280); assert.equal(download.readUInt32BE(20),960);
  await layout(390,844,true);
  await screenshot('functional-live-press-mobile','canvas');
  await evaluate(`document.querySelector('[data-mode="terrain"]').click()`); await done();
  await screenshot('functional-live-type-mobile','canvas');
  await call('Page.navigate', {url:'file://'+path.join(root,'docs/functional-postscript/index.html')});
  await evaluate(`new Promise(resolve=>{if(document.readyState==='complete')resolve();else addEventListener('load',resolve,{once:true})})`);
  await evaluate(`Promise.all([...document.images].map(i=>{i.loading='eager';return i.decode()}))`);
  await layout(390,844,true);
  assert.equal(await evaluate(`document.querySelectorAll('a[href^="live-studio.html"]').length>=4`),true);
  assert.equal(exceptions.length,0,JSON.stringify(exceptions));
  assert.ok(requests.every(u=>/^(file:|blob:|data:)/.test(u)),JSON.stringify(requests.filter(u=>/^https?:/.test(u))));
  report.checks = ['real local WASM typography execution','six distinct print treatments; automatic selection rerenders','eight bundled font choices; automatic selection rerenders','text edit changes glyph count and canvas',
    'palette changes rendered pixels','invalid input rejected before PS execution',
    'AOP inspection toggle changes only diagnostic mode','fault toggle changes actual entry/return/rejection counts',
    'changed proof text changes counts','watch / stop / restart','actual Blob PNG download',
    'desktop 1440x1100 and mobile 390x844 without horizontal overflow','introduction links and saved images',
    'zero JavaScript exceptions','zero page HTTP(S) requests'];
  report.source_sha256 = {};
  for (const name of ['live-studio.html','studio.js','studio.css','index.html']) {
    report.source_sha256[name] = createHash('sha256').update(await fs.readFile(path.join(root,'docs/functional-postscript',name))).digest('hex');
  }
  await fs.writeFile(path.join(root,'docs/functional-postscript/LIVE-BROWSER-VALIDATION.json'),JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify(report,null,2));
} finally { if(process.argv.includes('--close')) await call('Browser.close'); ws.close(); }
