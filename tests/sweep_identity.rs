//! Issue #891: every frame of a sweep renders byte-identically to a
//! standalone run of the same seed or value.
//!
//! `forall` over a dict's name keys follows the names' interned ids, and
//! ids are handed out in interning order, per thread. Two things used to
//! shift a frame's ids against a standalone run: the `--sweep NAME=`
//! preamble interning NAME before the program does (the #868 bug), and
//! frames sharing one thread's interner, so names an earlier frame made
//! with `cvn` were already taken when the next frame ran. The program below
//! makes 40 keys with `cvn` from strings that depend on the seed and the
//! swept value, then paints them in `forall` order, so either shift shows
//! up in the pixels. It uses no noise operators: a seed override also
//! seeds the noise table, which a standalone `S srand` does not.
//!
//! Standalone renders and sweep frames both write with `Pixmap::save_png`,
//! so the PNG bytes are compared directly.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod common;
use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn pscat(args: &[&str]) -> (bool, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn sheet(args: &[&str]) -> (bool, String) {
    let mut all = vec!["sheet"];
    all.extend(args);
    pscat(&all)
}

/// The program, with `srand`'s operand and `/Shade`'s default spliced in.
/// Dict `d` takes 40 seed-dependent `cvn` keys before `/Shade` is first
/// mentioned, so a preamble that interns `/Shade` early shifts all of them
/// by one id. Dict `e` then takes 40 keys that depend on `/Shade`, so a
/// frame that follows another value's frame on the same interner finds
/// some names already taken. (A single key dict whose keys follow the
/// `/Shade` mention keeps its `forall` order under the preamble's shift;
/// this shape was checked to differ under each cause on the old code.)
fn program(seed: i64, shade: i64) -> String {
    format!(
        "%!PS
/early pop
{seed} srand
/d 40 dict def
0 1 39 {{ /i exch def d rand 50 mod 100 mul i add 12 string cvs cvn i put }} for
/Shade where {{ pop }} {{ /Shade {shade} def }} ifelse
/e 40 dict def
0 1 39 {{ /i exch def e Shade 100 mul i add 1000000 add 12 string cvs cvn i put }} for
/c 0 def
d {{ exch pop 40 div setgray c 3 mul 0 3 50 rectfill /c c 1 add def }} forall
/c 0 def
e {{ exch pop 40 div setgray c 3 mul 50 3 50 rectfill /c c 1 add def }} forall
showpage
"
    )
}

fn write(dir: &Path, name: &str, source: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write source");
    path.to_str().expect("utf-8 path").to_string()
}

fn standalone(dir: &Path, tag: &str, seed: i64, shade: i64) -> Vec<u8> {
    let file = write(dir, &format!("{tag}.ps"), &program(seed, shade));
    let out = dir.join(format!("{tag}.png"));
    let (ok, e) = pscat(&["--page", "120x100", "--png", out.to_str().unwrap(), &file]);
    assert!(ok, "standalone {tag}: {e}");
    std::fs::read(out).expect("standalone png")
}

fn frame(prefix: &Path, n: usize) -> Vec<u8> {
    let stem = prefix.file_stem().unwrap().to_str().unwrap();
    std::fs::read(prefix.with_file_name(format!("{stem}-{n:03}.png"))).expect("frame png")
}

/// Frame 1 is the first on the thread, so only frames 2 and 3 could pick up
/// an earlier frame's names.
#[test]
fn every_sweep_seed_frame_matches_a_standalone_run_of_its_seed() {
    let dir = common::scratch_dir("sweep-identity-seed");
    let file = write(&dir, "swept.ps", &program(1, 1));
    let prefix: PathBuf = dir.join("frame.png");
    let (ok, e) = pscat(&[
        "--page",
        "120x100",
        "--sweep-seed",
        "1,2,3",
        "--png",
        prefix.to_str().unwrap(),
        &file,
    ]);
    assert!(ok, "{e}");
    for (n, seed) in [(1, 1), (2, 2), (3, 3)] {
        let want = standalone(&dir, &format!("seed{seed}"), seed, 1);
        assert!(
            want == frame(&prefix, n),
            "--sweep-seed frame {n} (seed={seed}) differs from a standalone `{seed} srand` run"
        );
    }
}

/// Frame 1 can only differ through the preamble's name ids; frames 2 and 3
/// through either cause.
#[test]
fn every_sweep_param_frame_matches_a_standalone_run_of_its_value() {
    let dir = common::scratch_dir("sweep-identity-param");
    let file = write(&dir, "swept.ps", &program(1, 9));
    let prefix: PathBuf = dir.join("frame.png");
    let (ok, e) = pscat(&[
        "--page",
        "120x100",
        "--sweep",
        "Shade=1,2,3",
        "--png",
        prefix.to_str().unwrap(),
        &file,
    ]);
    assert!(ok, "{e}");
    for (n, shade) in [(1, 1), (2, 2), (3, 3)] {
        let want = standalone(&dir, &format!("shade{shade}"), 1, shade);
        assert!(
            want == frame(&prefix, n),
            "--sweep frame {n} (Shade={shade}) differs from a standalone run with /Shade {shade}"
        );
    }
}

/// The 120x100 px image of cell `k` in a one-row `pscat sheet` of 120x100 pt
/// pages at 72 dpi with no row labels: 12 px margin, 1 px border, 14 px
/// column gap (the caption is narrower than the cell).
fn cell(p: &tiny_skia::Pixmap, k: u32) -> Vec<u8> {
    let x0 = 13 + k * (122 + 14);
    let mut out = Vec::new();
    for y in 13..113 {
        let i = ((y * p.width() + x0) * 4) as usize;
        out.extend_from_slice(&p.data()[i..i + 120 * 4]);
    }
    out
}

fn pixels(png: &[u8]) -> Vec<u8> {
    tiny_skia::Pixmap::decode_png(png)
        .expect("decode png")
        .data()
        .to_vec()
}

/// `pscat sheet` renders its cells one after another too, so each cell must
/// match the standalone render of its seed or value. A sheet is one PNG, so
/// cells are compared as decoded pixels.
#[test]
fn every_sheet_sweep_cell_matches_a_standalone_run() {
    let dir = common::scratch_dir("sweep-identity-sheet");
    let cases: [(&str, String, &[(i64, i64)]); 2] = [
        (
            "--sweep-seed",
            "1,2,3".to_string(),
            &[(1, 9), (2, 9), (3, 9)],
        ),
        (
            "--sweep",
            "Shade=1,2,3".to_string(),
            &[(1, 1), (1, 2), (1, 3)],
        ),
    ];
    for (flag, spec, frames) in cases {
        // An unswept Shade keeps the program's own default of 9.
        let file = write(&dir, "swept.ps", &program(1, 9));
        let out = dir.join("sheet.png");
        let (ok, e) = sheet(&[
            "-o",
            out.to_str().unwrap(),
            "--no-labels",
            "--no-sizes",
            "--page",
            "120x100",
            flag,
            &spec,
            &file,
        ]);
        assert!(ok, "{flag}: {e}");
        let p = tiny_skia::Pixmap::load_png(&out).expect("sheet png");
        for (k, &(seed, shade)) in frames.iter().enumerate() {
            let want = pixels(&standalone(
                &dir,
                &format!("sheet-{seed}-{shade}"),
                seed,
                shade,
            ));
            assert!(
                want == cell(&p, k as u32),
                "pscat sheet {flag} {spec}: cell {} (seed={seed}, Shade={shade}) differs \
                 from a standalone run",
                k + 1
            );
        }
    }
}
