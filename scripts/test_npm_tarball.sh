#!/usr/bin/env bash
# test_npm_tarball.sh — install a pscat npm package into a fresh project, the
# way a user would, and render with it (#566). The package is a packed
# tarball, or a version from the npm registry (#668):
#
#   ./scripts/build_npm.sh --pack DIR
#   ./scripts/test_npm_tarball.sh DIR/pscat-X.Y.Z.tgz [--no-types]
#   ./scripts/test_npm_tarball.sh pscat@X.Y.Z [--no-types] [--attested]
#
# An argument that is not a file is an npm spec, `pscat@X.Y.Z` or a bare
# X.Y.Z. For one, the installed version must be the one asked for and
# `npm audit signatures` must pass (the registry signature); --attested also
# requires the provenance attestation npm trusted publishing adds (so not for
# 0.6.2, which was published by hand). The release-check workflow runs this
# after `publish`. A published release before 0.7.0 is checked for the part of
# the API it had: no SVG/PDF, `pscat/node.js` (#1150), `pscat/pscat.global.js` (#1025) or type check.
#
# Checks: it installs with no install scripts; `import 'pscat'` resolves
# through the exports map to the Node entry and renders a page (pixels and
# output) and records SVG and PDF; the embedded lib/ loads; a PostScript error is a PostScriptError;
# `pscat/node.js` resolves (CJS and ESM) and imports (#1163); `pscat/pscat.wasm` and
# `pscat/package.json` resolve; `pscat/pscat.global.js` (#1025) parses as a classic script and
# renders through the `pscat` global it defines. Unless --no-types,
# a TypeScript consumer type-checks against index.d.ts (needs the network
# once, for the pinned typescript).
set -euo pipefail
TYPES=1
ATTESTED=0
ARG=""
for a in "$@"; do
  case "$a" in
    --no-types) TYPES=0 ;;
    --attested) ATTESTED=1 ;;
    --no-attested) ATTESTED=0 ;;
    -*) echo "test_npm_tarball.sh: unknown option $a" >&2; exit 2 ;;
    *) [[ -z "$ARG" ]] || { echo "test_npm_tarball.sh: one package only" >&2; exit 2; }; ARG="$a" ;;
  esac
done
[[ -n "$ARG" ]] || { echo "usage: test_npm_tarball.sh (FILE.tgz | pscat@X.Y.Z) [--no-types] [--attested]" >&2; exit 2; }
TYPESCRIPT=typescript@5.9.3

if [[ -f "$ARG" ]]; then
  SOURCE="the tarball"
  SPEC=$(cd "$(dirname "$ARG")" && pwd)/$(basename "$ARG")
  EXPECT=""
  [[ $ATTESTED -eq 0 ]] || { echo "--attested needs an npm spec: a tarball has no registry attestation" >&2; exit 2; }
else
  SOURCE="the registry"
  SPEC="$ARG"
  [[ "$SPEC" == pscat@* ]] || SPEC="pscat@$SPEC"
  EXPECT=""
  [[ "${SPEC#pscat@}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] && EXPECT="${SPEC#pscat@}"
fi

PROJ=$(mktemp -d "${TMPDIR:-/tmp}/pscat-npm-consumer.XXXXXX")
trap 'rm -rf "$PROJ"' EXIT
cd "$PROJ"
cat > package.json <<'EOF'
{ "name": "pscat-consumer", "version": "1.0.0", "private": true, "type": "module" }
EOF
npm install --no-audit --no-fund "$SPEC" >/dev/null

# A registry install: the signature (and, with --attested, the provenance)
# npm holds for the files it just installed. `npm audit signatures` lists a
# package with no attestation as informational and exits 0, so --attested
# reads its report.
if [[ "$SOURCE" == "the registry" ]]; then
  AUDIT=$(npm audit signatures 2>&1) || { echo "$AUDIT"; echo "npm audit signatures failed for $SPEC" >&2; exit 1; }
  echo "$AUDIT"
  grep -q "verified registry signature" <<<"$AUDIT" || { echo "no verified registry signature for $SPEC" >&2; exit 1; }
  if [[ $ATTESTED -eq 1 ]]; then
    grep -q "verified attestation" <<<"$AUDIT" || { echo "no verified provenance attestation for $SPEC" >&2; exit 1; }
  fi
fi

PSCAT_EXPECT_VERSION="$EXPECT" node - <<'EOF'
const pkg = JSON.parse(require('node:fs').readFileSync('node_modules/pscat/package.json', 'utf8'));
if (process.env.PSCAT_EXPECT_VERSION && pkg.version !== process.env.PSCAT_EXPECT_VERSION) {
  console.error(`installed pscat ${pkg.version}, expected ${process.env.PSCAT_EXPECT_VERSION}`);
  process.exit(1);
}
const hooks = ['preinstall', 'install', 'postinstall', 'prepare'].filter((k) => pkg.scripts && pkg.scripts[k]);
if (hooks.length) { console.error(`install scripts in the package: ${hooks}`); process.exit(1); }
EOF

cat > consumer.mjs <<'EOF'
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import vm from 'node:vm';
import { createRenderer, PostScriptError } from 'pscat';
import pkg from 'pscat/package.json' with { type: 'json' };

// A release from before 0.7.0 on the registry lacks the SVG/PDF output, the `pscat/node.js` export (#1150)
// and the classic script (#1025).
const [major, minor] = pkg.version.split('.').map(Number);
const legacy = process.env.PSCAT_SOURCE !== 'the tarball' && major === 0 && minor < 7;
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
if (!legacy) {
  r.render('0 0 1 setrgbcolor 10 10 50 50 rectfill', { width: 100, height: 100, svg: true, pdf: true });
  assert.match(r.svg(0), /^<svg /);
  assert.ok(new TextDecoder().decode(r.pdf().slice(0, 5)) === '%PDF-');
}
assert.ok(import.meta.resolve('pscat/pscat.wasm').endsWith('/node_modules/pscat/pscat.wasm'));
// The Node entry by its subpath (added in #1150, checked since #1163): CJS and ESM resolution both reach it, and it imports.
if (!legacy) {
  const cjs = createRequire(import.meta.url);
  // require.resolve gives a native path (backslashes on Windows); import.meta.resolve a file: URL.
  assert.ok(cjs.resolve('pscat/node.js').replaceAll('\\', '/').endsWith('/node_modules/pscat/node.js'));
  assert.ok(import.meta.resolve('pscat/node.js').endsWith('/node_modules/pscat/node.js'));
  const nodeEntry = await import('pscat/node.js');
  assert.equal(typeof nodeEntry.createRenderer, 'function');
}
// The classic-script build: a script that is not a module (vm.Script would throw on one).
if (!legacy) {
  const globalSrc = await readFile(new URL(import.meta.resolve('pscat/pscat.global.js')), 'utf8');
  assert.doesNotThrow(() => new vm.Script(globalSrc));
  vm.runInThisContext(globalSrc);
  const wasm = await readFile(new URL(import.meta.resolve('pscat/pscat.wasm')));
  const g = await globalThis.pscat.createRenderer({ wasm });
  assert.equal(g.render('(classic) print', { width: 10, height: 10 }).output, 'classic');
} else {
  console.log(`pscat ${pkg.version} predates pscat.global.js (#1025): classic-script check skipped`);
}
console.log(`pscat ${pkg.version} from ${process.env.PSCAT_SOURCE}: rendered ${img.width}x${img.height}, ${blue} blue pixels`);
EOF
PSCAT_SOURCE="$SOURCE" node consumer.mjs

[[ $TYPES -eq 1 ]] || exit 0
if [[ "$SOURCE" == "the registry" ]] && node -e 'const [a, b] = require("./node_modules/pscat/package.json").version.split(".").map(Number); process.exit(a === 0 && b < 7 ? 0 : 1)'; then
  echo "types: skipped, a release before 0.7.0 has no SVG/PDF in index.d.ts"
  exit 0
fi
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
r.render('0 0 10 10 rectfill', { svg: true, pdf: true });
const svgs: string[] = r.svgPages();
const first: string = r.svg(0);
const pdf: Uint8Array = r.pdf();
void svgs; void first; void pdf;
// @ts-expect-error the page options are numbers
r.render('', { width: '100' });
void data; void more;
EOF
npx tsc -p .
echo "types: index.d.ts checks with $TYPESCRIPT"
