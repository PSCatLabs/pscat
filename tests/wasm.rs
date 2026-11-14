//! The Stage 16 browser build: compile the interpreter to
//! wasm32-unknown-unknown and drive it under node through the real
//! JS library (web/pscat.js) — instantiate, run a program, count
//! pixels, exercise the error path — and compare its pages with the
//! native renderer's (#566: the npm package ships this module). Skips
//! gracefully (gs-test style) when the wasm target or node isn't
//! installed; the `npm` workflow installs both, so CI runs it there.

use std::process::Command;

use common::CommandDeadline as _;

mod common;

fn have(cmd: &str, args: &[&str], needle: &str) -> bool {
    Command::new(cmd)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(needle))
}

/// Build web/pscat.wasm once per test process; false (skip) without the
/// wasm target or node.
fn wasm_ready() -> bool {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *READY.get_or_init(build_wasm)
}

fn build_wasm() -> bool {
    if !have(
        "rustup",
        &["target", "list", "--installed"],
        "wasm32-unknown-unknown",
    ) {
        eprintln!("skipping wasm test: wasm32-unknown-unknown target not installed");
        return false;
    }
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("skipping wasm test: node not installed");
        return false;
    }
    let build = Command::new(root().join("scripts/build_wasm.sh"))
        .output()
        .expect("run build_wasm.sh");
    assert!(
        build.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    true
}

fn root() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn wasm_build_runs_under_node() {
    if !wasm_ready() {
        return;
    }
    let root = root();

    let driver = format!(
        r#"
import {{ readFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
const ps = await Pscat.instantiate(readFileSync('{wasm}'));
const rc = ps.run('0.2 0.4 0.9 setrgbcolor 10 10 80 80 rectfill showpage', 100, 100);
if (rc !== 0) {{ console.error('run failed: ' + ps.error); process.exit(1); }}
if (ps.width !== 100 || ps.height !== 100) {{ console.error('bad page size'); process.exit(1); }}
const px = ps.pixels();
let blue = 0;
for (let i = 0; i < px.length; i += 4) if (px[i + 2] > 180 && px[i] < 120) blue++;
if (blue < 3000) {{ console.error('too few blue pixels: ' + blue); process.exit(1); }}
// The error path: report comes back, the session object stays usable.
const rc2 = ps.run('1 2 bogus', 100, 100);
if (rc2 !== -1) {{ console.error('expected error status, got ' + rc2); process.exit(1); }}
if (!ps.error.includes('undefined')) {{ console.error('error not named: ' + ps.error); process.exit(1); }}
if (ps.run('0 0 50 50 rectfill', 100, 100) !== 0) {{ console.error('not reusable after error'); process.exit(1); }}
// What the program prints comes back as .output (#252), fresh per run.
if (ps.output !== '') {{ console.error('output not cleared: ' + ps.output); process.exit(1); }}
ps.run('3 4 add == (hi) print', 100, 100);
if (ps.output !== '7\nhi') {{ console.error('wrong output: ' + JSON.stringify(ps.output)); process.exit(1); }}
// The libraries are embedded (#598): wasm has no disk, yet run finds them.
if (ps.run('(lib/artkit.ps) run (lib/fonts/neon.ps) run /frnd where {{ pop (ok) print }} if', 100, 100) !== 0 || ps.output !== 'ok') {{
  console.error('embedded lib/ did not load: ' + ps.error + ' ' + JSON.stringify(ps.output)); process.exit(1);
}}
console.log('WASM_OK ' + blue);
"#,
        lib = root.join("web/pscat.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
    );
    let dir = common::scratch_dir("pscat-wasm");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("smoke.mjs");
    std::fs::write(&script, driver).expect("write driver");

    let out = Command::new("node")
        .arg(&script)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("WASM_OK"),
        "node smoke test failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Programs the wasm build must render as the native build does: fills
/// and curves, strokes under rotation (trig), a built-in font, an
/// embedded library, and a scaled (144 dpi) page. (width, height,
/// scale, source.)
const SAME_AS_NATIVE: &[(u32, u32, f32, &str)] = &[
    (
        120,
        100,
        1.0,
        "newpath 60 50 40 0 360 arc 0.8 0.2 0.1 setrgbcolor fill \
         0 setgray 3 setlinewidth 10 10 moveto 110 80 lineto 20 90 70 20 100 95 curveto stroke",
    ),
    (
        100,
        100,
        1.0,
        "0 10 350 { gsave 50 50 translate rotate 0 0 moveto 45 0 lineto stroke grestore } for",
    ),
    (
        200,
        60,
        1.0,
        "/Times-Bold findfont 30 scalefont setfont 5 20 moveto (Hamburgefons) show \
         /Courier findfont 12 scalefont setfont 5 5 moveto (0123456789) show",
    ),
    (
        100,
        100,
        1.0,
        "(lib/artkit.ps) run 7 srand 0 1 40 { pop frnd 100 mul frnd 100 mul 3 0 360 arc fill } for",
    ),
    (
        60,
        50,
        2.0,
        "0.1 0.5 0.3 setrgbcolor 10 10 moveto 50 40 lineto 30 5 lineto closepath fill",
    ),
];

/// The same programs through wasm (under node) and natively: the pages
/// must match. Both builds run the same Rust, so they agree exactly
/// except where a float math function rounds differently between the
/// native libm and Rust's wasm one; the tolerance below allows that
/// (a few pixels per page, by a few levels), nothing more.
#[test]
fn wasm_pages_match_the_native_renderer() {
    if !wasm_ready() {
        return;
    }
    let root = root();
    let dir = common::scratch_dir("pscat-wasm-native");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let jobs: Vec<String> = SAME_AS_NATIVE
        .iter()
        .enumerate()
        .map(|(i, (w, h, scale, src))| {
            format!(
                "[{w}, {h}, {scale}, {src:?}, {out:?}]",
                out = dir.join(format!("page-{i}.rgba")).display().to_string()
            )
        })
        .collect();
    let driver = format!(
        r#"
import {{ readFileSync, writeFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
const ps = await Pscat.instantiate(readFileSync('{wasm}'));
for (const [w, h, scale, src, out] of [{jobs}]) {{
  if (ps.run(src, w, h, scale) !== 0) {{ console.error(src + ': ' + ps.error); process.exit(1); }}
  writeFileSync(out, ps.pixels());
}}
"#,
        lib = root.join("web/pscat.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
        jobs = jobs.join(", "),
    );
    let script = dir.join("pages.mjs");
    std::fs::write(&script, driver).expect("write driver");
    let out = Command::new("node")
        .arg(&script)
        .output()
        .expect("run node");
    assert!(
        out.status.success(),
        "node driver failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    for (i, (w, h, scale, src)) in SAME_AS_NATIVE.iter().enumerate() {
        let wasm = std::fs::read(dir.join(format!("page-{i}.rgba"))).expect("wasm page");
        let mut it = pscat::Interp::with_page_scaled(*w, *h, *scale).expect("page");
        it.run_str(src)
            .unwrap_or_else(|e| panic!("native {src:?}: {e}"));
        let native = it.gfx().pixmap.data();
        assert_eq!(wasm.len(), native.len(), "page {i}: size differs");
        let inked = native
            .chunks(4)
            .filter(|p| p[..3] != [255, 255, 255])
            .count();
        assert!(
            inked > 100,
            "page {i} is nearly blank ({inked} pixels): a vacuous match"
        );
        let differing = wasm
            .chunks(4)
            .zip(native.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        let worst = wasm
            .iter()
            .zip(native)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        let pixels = native.len() / 4;
        eprintln!("page {i}: {differing} of {pixels} pixels differ, by at most {worst}");
        assert!(
            differing * 200 <= pixels && worst <= 8,
            "page {i} ({src:?}): {differing} of {pixels} pixels differ from native, by up to {worst}"
        );
    }
}

/// A two-page document for the vector tests (#1003): text in two built-in
/// faces, an arc, strokes under rotation (trig, where native and wasm libm
/// could round apart), a curve, and the DSC comments the CLI turns into
/// the PDF's /Info. No %%BoundingBox: `--page` sets the page, as in wasm.
const VECTOR_DOC: &str = "%!PS-Adobe-3.0
%%Title: Vector parity
%%For: pscat tests
/Times-Roman findfont 24 scalefont setfont
72 700 moveto (Page one: Hamburgefons) show
newpath 300 400 120 0 360 arc 0.8 0.2 0.1 setrgbcolor fill
0 setgray 2 setlinewidth
0 15 345 { gsave 300 400 translate rotate 0 0 moveto 150 0 lineto stroke grestore } for
showpage
/Helvetica-Bold findfont 18 scalefont setfont
72 700 moveto (Page two) show
0.1 0.5 0.3 setrgbcolor 100 100 moveto 400 300 lineto 200 50 lineto closepath fill
gsave 37 rotate 200 200 translate 0 0 50 30 rectstroke grestore
100 500 moveto 150 600 250 450 300 550 curveto stroke
gsave newpath 400 500 80 0 360 arc clip
<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [380 520 5 400 500 80]
   /Function << /FunctionType 3 /Domain [0 1] /Bounds [0.5] /Encode [0 1 0 1]
     /Functions [ << /FunctionType 2 /Domain [0 1] /C0 [1 1 0.8] /C1 [0.9 0.4 0.1] /N 1 >>
                  << /FunctionType 2 /Domain [0 1] /C0 [0.9 0.4 0.1] /C1 [0.2 0 0.3] /N 1 >> ] >> >> shfill
grestore
showpage
";

/// SVG and PDF from the wasm build equal `pscat --svg`/`--pdf` byte for
/// byte (#1003), through the raw C ABI wrapper (web/pscat.js) and through
/// the npm package's Renderer, at 72 and 144 dpi; and a run is
/// repeatable: twice in one module instance, and in a fresh one.
#[test]
fn wasm_vector_output_matches_the_cli() {
    if !wasm_ready() {
        return;
    }
    let root = root();
    let dir = common::scratch_dir("pscat-wasm-vector");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let doc = dir.join("doc.ps");
    std::fs::write(&doc, VECTOR_DOC).expect("write doc");

    for dpi in [72, 144] {
        let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .args(["--page", "612x792", "--dpi", &dpi.to_string()])
            .arg("--svg")
            .arg(dir.join(format!("native-{dpi}.svg")))
            .arg("--pdf")
            .arg(dir.join(format!("native-{dpi}.pdf")))
            .arg(&doc)
            .output_deadline()
            .expect("run pscat");
        assert!(
            out.status.success(),
            "pscat --svg/--pdf failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // The npm Renderer imports ./core.js, the copy of web/pscat.js that
    // build_npm.sh makes; give it one here without building the package.
    let npm = dir.join("npm");
    std::fs::create_dir_all(&npm).expect("npm dir");
    std::fs::copy(root.join("npm/renderer.js"), npm.join("renderer.js")).expect("renderer.js");
    std::fs::copy(root.join("web/pscat.js"), npm.join("core.js")).expect("core.js");

    let driver = format!(
        r#"
import {{ readFileSync, writeFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
import {{ instantiate }} from 'file://{renderer}';
const bytes = readFileSync('{wasm}');
const src = readFileSync('{doc}');
const out = (name, data) => writeFileSync('{dir}/' + name, data);
// Raw ABI: instances a and b, two runs each, at both resolutions.
for (const inst of ['a', 'b']) {{
  const ps = await Pscat.instantiate(bytes);
  for (const run of [1, 2]) for (const dpi of [72, 144]) {{
    if (ps.run(src, 612, 792, dpi / 72, {{ svg: true, pdf: true }}) !== 0) {{ console.error(ps.error); process.exit(1); }}
    const n = ps.svgCount();
    if (n !== 2) {{ console.error('svgCount ' + n); process.exit(1); }}
    if (ps.pageCount() !== n) {{ console.error('pageCount ' + ps.pageCount()); process.exit(1); }}
    for (let i = 0; i < n; i++) out(`abi-${{inst}}${{run}}-${{dpi}}-${{i + 1}}.svg`, ps.svg(i));
    out(`abi-${{inst}}${{run}}-${{dpi}}.pdf`, ps.pdf());
    if (ps.svg(n) !== null) {{ console.error('page past the end'); process.exit(1); }}
  }}
}}
// Recording is opt-in: a plain run has neither.
const plain = await Pscat.instantiate(bytes);
plain.run(src, 612, 792);
if (plain.svgCount() !== 0 || plain.svg(0) !== null || plain.pdf() !== null) {{
  console.error('a plain run recorded vectors'); process.exit(1);
}}
// ...but the page count does not depend on recording (#1011).
if (plain.pageCount() !== 2) {{ console.error('plain pageCount ' + plain.pageCount()); process.exit(1); }}
// The npm API, over the same module.
const r = await instantiate(await WebAssembly.compile(bytes));
for (const dpi of [72, 144]) {{
  r.render(src, {{ width: 612, height: 792, dpi, svg: true, pdf: true }});
  r.svgPages().forEach((svg, i) => out(`npm-${{dpi}}-${{i + 1}}.svg`, svg));
  out(`npm-${{dpi}}.pdf`, r.pdf());
}}
console.log('VECTOR_OK');
"#,
        lib = root.join("web/pscat.js").display(),
        renderer = npm.join("renderer.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
        doc = doc.display(),
        dir = dir.display(),
    );
    let script = dir.join("vector.mjs");
    std::fs::write(&script, driver).expect("write driver");
    let out = Command::new("node")
        .arg(&script)
        .output_deadline()
        .expect("run node");
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("VECTOR_OK"),
        "node vector driver failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read = |name: String| {
        std::fs::read(dir.join(&name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
    };
    for dpi in [72, 144] {
        let pdf = read(format!("native-{dpi}.pdf"));
        assert!(pdf.starts_with(b"%PDF-"), "native PDF at {dpi} dpi");
        let title = b"/Title (Vector parity)";
        assert!(
            pdf.windows(title.len()).any(|w| w == title),
            "the CLI's PDF lost its %%Title"
        );
        let mut produced = vec![format!("npm-{dpi}.pdf")];
        for run in ["a1", "a2", "b1", "b2"] {
            produced.push(format!("abi-{run}-{dpi}.pdf"));
        }
        for name in produced {
            assert!(
                read(name.clone()) == pdf,
                "{name} differs from pscat --pdf at {dpi} dpi"
            );
        }
        for page in 1..=2 {
            let svg = read(format!("native-{dpi}-{page:03}.svg"));
            assert!(svg.starts_with(b"<svg"), "native SVG page {page}");
            let mut produced = vec![format!("npm-{dpi}-{page}.svg")];
            for run in ["a1", "a2", "b1", "b2"] {
                produced.push(format!("abi-{run}-{dpi}-{page}.svg"));
            }
            for name in produced {
                assert!(
                    read(name.clone()) == svg,
                    "{name} differs from pscat --svg page {page} at {dpi} dpi"
                );
            }
        }
    }
}

/// Standard-font text in several sizes, styles and angles, for the
/// `--svg-text` parity check (#1119).
const TEXT_DOC: &str = "%!PS
/Times-Roman findfont 24 scalefont setfont 72 700 moveto (Selectable <text> & more) show
/Helvetica-Bold findfont 14 scalefont setfont
0 30 330 { gsave 300 400 translate rotate 40 0 moveto 0.2 0.3 0.6 setrgbcolor (Hamburgefons) show grestore } for
/Courier-Oblique findfont [10 0 3 12 0 0] makefont setfont 72 100 moveto (skewed and stretched) show
showpage
";

/// `--svg-text` SVG from the wasm build equals the CLI's byte for byte,
/// through the raw ABI (`record.svgText`) and the npm Renderer
/// (`svgText: true`), at 72 and 144 dpi; and the flag really changes the
/// output (#1119).
#[test]
fn wasm_svg_text_matches_the_cli() {
    if !wasm_ready() {
        return;
    }
    let root = root();
    let dir = common::scratch_dir("pscat-wasm-svg-text");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let doc = dir.join("doc.ps");
    std::fs::write(&doc, TEXT_DOC).expect("write doc");
    for dpi in [72, 144] {
        let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .args(["--page", "612x792", "--dpi", &dpi.to_string(), "--svg-text"])
            .arg("--svg")
            .arg(dir.join(format!("native-{dpi}.svg")))
            .arg(&doc)
            .output_deadline()
            .expect("run pscat");
        assert!(
            out.status.success(),
            "pscat --svg-text failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let npm = dir.join("npm");
    std::fs::create_dir_all(&npm).expect("npm dir");
    std::fs::copy(root.join("npm/renderer.js"), npm.join("renderer.js")).expect("renderer.js");
    std::fs::copy(root.join("web/pscat.js"), npm.join("core.js")).expect("core.js");
    let driver = format!(
        r#"
import {{ readFileSync, writeFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
import {{ instantiate }} from 'file://{renderer}';
const bytes = readFileSync('{wasm}');
const src = readFileSync('{doc}');
const out = (name, data) => writeFileSync('{dir}/' + name, data);
const ps = await Pscat.instantiate(bytes);
for (const dpi of [72, 144]) {{
  if (ps.run(src, 612, 792, dpi / 72, {{ svg: true, svgText: true }}) !== 0) {{ console.error(ps.error); process.exit(1); }}
  out(`abi-${{dpi}}.svg`, ps.svg(0));
  if (ps.run(src, 612, 792, dpi / 72, {{ svg: true }}) !== 0) {{ console.error(ps.error); process.exit(1); }}
  out(`abi-paths-${{dpi}}.svg`, ps.svg(0));
}}
const r = await instantiate(await WebAssembly.compile(bytes));
for (const dpi of [72, 144]) {{
  r.render(src, {{ width: 612, height: 792, dpi, svg: true, svgText: true }});
  out(`npm-${{dpi}}.svg`, r.svg(0));
}}
try {{ r.render(src, {{ svgText: true }}); console.error('svgText without svg was accepted'); process.exit(1); }}
catch (e) {{ if (!(e instanceof TypeError)) throw e; }}
console.log('TEXT_OK');
"#,
        lib = root.join("web/pscat.js").display(),
        renderer = npm.join("renderer.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
        doc = doc.display(),
        dir = dir.display(),
    );
    let script = dir.join("text.mjs");
    std::fs::write(&script, driver).expect("write driver");
    let out = Command::new("node")
        .arg(&script)
        .output_deadline()
        .expect("run node");
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("TEXT_OK"),
        "node svg-text driver failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |name: String| {
        std::fs::read_to_string(dir.join(&name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
    };
    for dpi in [72, 144] {
        let native = read(format!("native-{dpi}.svg"));
        assert!(
            native.contains("<text "),
            "the CLI wrote no <text> at {dpi} dpi"
        );
        for name in [format!("abi-{dpi}.svg"), format!("npm-{dpi}.svg")] {
            assert!(
                read(name.clone()) == native,
                "{name} differs from pscat --svg-text at {dpi} dpi"
            );
        }
        assert!(
            !read(format!("abi-paths-{dpi}.svg")).contains("<text "),
            "a run without svgText wrote <text>"
        );
    }
}

/// Run a node driver (an ES module body that has `ps`, `fail` and the
/// bomb helpers in scope) against the real wasm module; the driver prints
/// `OK` when every check passed.
fn run_cap_driver(name: &str, body: &str) {
    let root = root();
    let driver = format!(
        r#"
import {{ readFileSync }} from 'node:fs';
import zlib from 'node:zlib';
import {{ Pscat }} from 'file://{lib}';
const ps = await Pscat.instantiate(readFileSync('{wasm}'));
const fail = (m) => {{ console.error(m); process.exit(1); }};
// Zeros, MIB mebibytes of them, deflated `layers` times, streamed so the
// expanded size never sits in memory. Returns the compressed bytes.
async function bombBytes(MIB, layers) {{
  let s = (async function* () {{ const z = Buffer.alloc(1 << 20); for (let i = 0; i < MIB; i++) yield z; }})();
  const {{ Readable }} = await import('node:stream');
  let r = Readable.from(s);
  for (let i = 0; i < layers; i++) r = r.pipe(zlib.createDeflate({{ level: i === 0 ? 1 : 9 }}));
  const parts = [];
  for await (const c of r) parts.push(c);
  return Buffer.concat(parts);
}}
// currentfile's rest is the bomb: `layers` filters, then one operator
// (flushfile) drains them all.
function bombProgram(data, layers, before = '') {{
  return Buffer.concat([
    Buffer.from(before + 'currentfile ' + '/FlateDecode filter '.repeat(layers) + 'flushfile\n'),
    data, Buffer.from('\n')]);
}}
{body}
console.log('OK');
"#,
        lib = root.join("web/pscat.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
    );
    let dir = common::scratch_dir(name);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("cap.mjs");
    std::fs::write(&script, driver).expect("write driver");
    let out = Command::new("node")
        .arg(&script)
        .output_deadline()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim_end().ends_with("OK"),
        "node step-cap driver failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The browser build has no clock, so `maxSteps` (`ps_set_max_steps`) is
/// what stops a program there, and since #1208 it also stops one operator
/// draining a decompression bomb, because decoded filter output counts a
/// step per KiB. A 600-byte, two-layer FlateDecode bomb that expands to
/// 32 MiB in a single `flushfile` must give `limitexceeded (steps)` within
/// a few KiB of the cap and not run to the end. The cap is off until set,
/// applies from the moment it is set (to the program in progress too) and
/// to the programs begun after it, and set before each `step()` it bounds
/// that call, which is how the playground uses it (#1210).
#[test]
fn wasm_step_cap_stops_a_single_operator_decompression_bomb() {
    if !wasm_ready() {
        return;
    }
    run_cap_driver(
        "pscat-wasm-step-cap",
        r#"
const MIB = 32;
const data = await bombBytes(MIB, 2);
if (data.length > 2000) fail('bomb not small: ' + data.length);
const src = bombProgram(data, 2);
const drain = () => { ps.begin(src, 100, 100); return ps.step(1 << 16); };

// No cap set: the whole bomb is drained, and nothing is counted.
if (ps.stepsUsed !== 0) fail('steps counted before any program');
let st = drain();
if (st !== 0) fail('uncapped drain failed: ' + st + ' ' + ps.error);
if (ps.stepsUsed !== 0) fail('steps counted with no cap: ' + ps.stepsUsed);

// A cap of 1000, set before begin, stops it after about 1 MiB of the 32,
// inside the one operator. (The count lags the cap by the KiB the stopping
// call had charged but not yet handed over.)
ps.maxSteps = 1000;
const t = Date.now();
st = drain();
const ms = Date.now() - t;
if (st !== -1 || !ps.error.includes('limitexceeded (steps)')) fail('not stopped: ' + st + ' ' + ps.error);
if (ps.stepsUsed < 500 || ps.stepsUsed > 1400) fail('stopped far from the cap: ' + ps.stepsUsed);
if (ms > 5000) fail('stopped but slowly: ' + ms + ' ms');
// The page and the session survive: the next program runs clean.
if (ps.run('0 0 10 10 rectfill', 100, 100) !== 0) fail('not reusable after the limit: ' + ps.error);

// A cap with room for the whole 32 MiB (32768 KiB) lets it finish, and counts it.
ps.maxSteps = 100000;
st = drain();
if (st !== 0) fail('roomy cap stopped it: ' + ps.error);
if (ps.stepsUsed < 32768 || ps.stepsUsed > 33500) fail('miscounted: ' + ps.stepsUsed);

// An endless loop, which has no operator to drain, stops the same way.
ps.maxSteps = 100000;
ps.begin('{ } loop', 100, 100);
let more;
do { more = ps.step(5000); } while (more === 1);
if (more !== -1 || !ps.error.includes('limitexceeded (steps)')) fail('loop not stopped: ' + more + ' ' + ps.error);

// Set again before each step, the cap is per call: a program of 600,000
// steps runs to the end under a 5,000-step cap, because no one call comes
// near it ...
ps.maxSteps = 0;
ps.begin('0 1 200000 { pop } for 0 0 10 10 rectfill', 100, 100);
let calls = 0;
do { ps.maxSteps = 5000; more = ps.step(1000); calls++; } while (more === 1);
if (more !== 0 || calls < 100) fail('per-call cap stopped a small-step program: ' + more + ' ' + ps.error + ' after ' + calls);
// ... and the bomb, after a long run of those calls, is stopped in its frame.
ps.maxSteps = 0;
ps.begin(bombProgram(data, 2, '0 1 100000 { pop } for\n'), 100, 100);
calls = 0;
do { ps.maxSteps = 5000; more = ps.step(1000); calls++; } while (more === 1);
if (more !== -1 || !ps.error.includes('limitexceeded (steps)') || !ps.error.includes('flushfile')) fail('frame cap missed the bomb: ' + more + ' ' + ps.error);
if (calls < 50) fail('stopped before the loop was over: ' + calls);

// 0 lifts the cap again; a bad value is refused.
ps.maxSteps = 0;
st = drain();
if (st !== 0 || ps.stepsUsed !== 0) fail('cap not lifted: ' + st + ' ' + ps.stepsUsed);
for (const bad of [-1, 1.5, NaN, Infinity]) {
  let threw = false;
  try { ps.maxSteps = bad; } catch (e) { threw = e instanceof RangeError; }
  if (!threw) fail('maxSteps accepted ' + bad);
}
"#,
    );
}

/// The number the playground ships, from `site/playground.html`: it must
/// stop a decompression bomb in its frame, and let a frame of ordinary
/// work through. A three-layer, 560 MiB bomb of a few hundred bytes is
/// drained by one `flushfile`; at a step per KiB it passes 500,000 steps
/// after about 500 MiB (#1210).
#[test]
fn wasm_playground_frame_cap_stops_a_bomb_and_passes_a_frame() {
    if !wasm_ready() {
        return;
    }
    let html = std::fs::read_to_string(root().join("site/playground.html")).expect("playground");
    let cap: u64 = html
        .lines()
        .find_map(|l| l.trim().strip_prefix("const DEFAULT_FRAME_MAX_STEPS = "))
        .map(|v| v.trim_end_matches(';').replace('_', ""))
        .expect("DEFAULT_FRAME_MAX_STEPS in site/playground.html")
        .parse()
        .expect("a whole number");
    assert!(
        html.contains("ps.maxSteps = FRAME_MAX_STEPS;"),
        "the playground must set the cap before each step"
    );
    run_cap_driver(
        "pscat-wasm-playground-cap",
        &format!(
            r#"
const CAP = {cap};
const MIB = Math.ceil(CAP / 1024 * 1.15); // a bomb bigger than the cap allows, but cheap to make
const data = await bombBytes(MIB, 3);
if (data.length > 5000) fail('bomb not small: ' + data.length);
ps.maxSteps = 0;
ps.begin(bombProgram(data, 3, '0 0 50 50 rectfill\n'), 100, 100);
const t = Date.now();
let more, calls = 0;
do {{ ps.maxSteps = CAP; more = ps.step(220); calls++; }} while (more === 1);
const ms = Date.now() - t;
if (more !== -1 || !ps.error.includes('limitexceeded (steps)')) fail('the default cap missed the bomb: ' + more + ' ' + ps.error);
if (ms > 30000) fail('stopped, but after ' + ms + ' ms');
// The page it had drawn stays.
const px = ps.pixels();
if (px[(99 * 100 + 10) * 4] > 100) fail('the page it had drawn is gone');
// A frame at the slider's top speed is nowhere near the cap.
ps.maxSteps = CAP;
ps.begin('0 1 100000 {{ pop }} for', 100, 100);
ps.maxSteps = CAP;
ps.step(4000);
if (ps.stepsUsed > CAP / 50) fail('a frame charged ' + ps.stepsUsed);
console.log('stopped after ' + ms + ' ms, ' + calls + ' calls');
"#
        ),
    );
}
