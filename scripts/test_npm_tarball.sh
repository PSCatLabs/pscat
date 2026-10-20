#!/usr/bin/env bash
# test_npm_tarball.sh — install a packed pscat npm tarball into a fresh
# project, the way a user would, and render with it (#566).
#
#   ./scripts/build_npm.sh --pack DIR
#   ./scripts/test_npm_tarball.sh DIR/pscat-X.Y.Z.tgz [--no-types]
#
# Checks: it installs with no install scripts; `import 'pscat'` resolves
# through the exports map to the Node entry and renders a page (pixels and
# output); the embedded lib/ loads; a PostScript error is a PostScriptError;
# `pscat/pscat.wasm` and `pscat/package.json` resolve. Unless --no-types,
# a TypeScript consumer type-checks against index.d.ts (needs the network
# once, for the pinned typescript).
set -euo pipefail
TGZ=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
TYPES=1
[[ "${2:-}" == "--no-types" ]] && TYPES=0
TYPESCRIPT=typescript@5.9.3

PROJ=$(mktemp -d "${TMPDIR:-/tmp}/pscat-npm-consumer.XXXXXX")
trap 'rm -rf "$PROJ"' EXIT
cd "$PROJ"
cat > package.json <<'EOF'
{ "name": "pscat-consumer", "version": "1.0.0", "private": true, "type": "module" }
EOF
npm install --no-audit --no-fund "$TGZ" >/dev/null

node - <<'EOF'
const pkg = JSON.parse(require('node:fs').readFileSync('node_modules/pscat/package.json', 'utf8'));
const hooks = ['preinstall', 'install', 'postinstall', 'prepare'].filter((k) => pkg.scripts && pkg.scripts[k]);
if (hooks.length) { console.error(`install scripts in the package: ${hooks}`); process.exit(1); }
EOF

cat > consumer.mjs <<'EOF'
import assert from 'node:assert/strict';
import { createRenderer, PostScriptError } from 'pscat';
import pkg from 'pscat/package.json' with { type: 'json' };

const r = await createRenderer();
const img = r.render('0.2 0.4 0.9 setrgbcolor 10 10 80 80 rectfill ' +
  '/Times-Roman findfont 20 scalefont setfont 0 setgray 10 92 moveto (pscat) show (ok) print',
  { width: 100, height: 120 });
assert.equal(img.width, 100);
assert.equal(img.height, 120);
assert.equal(img.output, 'ok');
let blue = 0, black = 0;
for (let i = 0; i < img.data.length; i += 4) {
  if (img.data[i + 2] > 180 && img.data[i] < 120) blue++;
  if (img.data[i] + img.data[i + 1] + img.data[i + 2] < 150) black++;
}
assert.ok(blue > 6000, `blue ${blue}`);
assert.ok(black > 20, `text ${black}`);
assert.equal(r.render('(lib/mathkit.ps) run (lib) print', { width: 10, height: 10 }).output, 'lib');
assert.throws(() => r.render('nosuchop'), (e) => e instanceof PostScriptError && e.errorName === 'undefined');
assert.ok(import.meta.resolve('pscat/pscat.wasm').endsWith('/node_modules/pscat/pscat.wasm'));
console.log(`pscat ${pkg.version} from the tarball: rendered ${img.width}x${img.height}, ${blue} blue pixels`);
EOF
node consumer.mjs

[[ $TYPES -eq 1 ]] || exit 0
npm install --no-audit --no-fund --no-save "$TYPESCRIPT" >/dev/null
cat > tsconfig.json <<'EOF'
{ "compilerOptions": { "module": "nodenext", "target": "es2022", "lib": ["es2022", "dom"],
  "strict": true, "noEmit": true, "skipLibCheck": false, "types": [] }, "files": ["consumer.ts"] }
EOF
cat > consumer.ts <<'EOF'
import { createRenderer, PostScriptError, type RenderResult, type Renderer } from 'pscat';

const r: Renderer = await createRenderer({ wasm: new URL('https://example.com/pscat.wasm') });
const img: RenderResult = r.render('0 0 10 10 rectfill', { width: 20, height: 20, dpi: 144 });
const data: Uint8ClampedArray = img.data;
for (const n of r.frames('1 1 add', { budget: 10 })) {
  const frame: number = n;
  void frame;
}
const more: boolean = r.step(100);
r.addFont('EBGaramond-Regular', new Uint8Array(0));
try {
  r.render('bogus');
} catch (e) {
  if (e instanceof PostScriptError) {
    const name: string = e.errorName;
    const line: number | undefined = e.line;
    void name; void line;
  }
}
// @ts-expect-error the page options are numbers
r.render('', { width: '100' });
void data; void more;
EOF
npx tsc -p .
echo "types: index.d.ts checks with $TYPESCRIPT"
