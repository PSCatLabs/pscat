//! The Stage 16 browser build: compile the interpreter to
//! wasm32-unknown-unknown and drive it under node through the real
//! JS library (web/pscat.js) — instantiate, run a program, count
//! pixels, exercise the error path — and compare its pages with the
//! native renderer's (#566: the npm package ships this module). Skips
//! gracefully (gs-test style) when the wasm target or node isn't
//! installed; the `npm` workflow installs both, so CI runs it there.

use std::process::Command;

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
