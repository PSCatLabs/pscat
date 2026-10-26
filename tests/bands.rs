//! `pscat --bands N` (issue #736): a banded render is the plain `--png`
//! render, byte for byte, and the compositor puts each band's rows where
//! they belong.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tiny_skia::Pixmap;

mod common;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Run pscat with `args` (cwd: the crate root, so `(lib/...) run` resolves).
fn pscat(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write_source(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write source");
    path
}

/// Render `file` plainly and with each band count, and require identical
/// PNG bytes. `extra` goes on every command line.
fn assert_bands_match(tag: &str, file: &str, extra: &[&str], counts: &[u32]) {
    let dir = common::scratch_dir(tag);
    let plain = dir.join("plain.png");
    let mut args: Vec<&str> = extra.to_vec();
    let plain_s = plain.to_str().expect("utf-8 path").to_string();
    args.extend(["--png", &plain_s, file]);
    let (ok, _, stderr) = pscat(&args);
    assert!(ok, "{tag}: plain render failed: {stderr}");
    let want = std::fs::read(&plain).expect("plain png");
    for &n in counts {
        let out = dir.join(format!("bands{n}.png"));
        let out_s = out.to_str().expect("utf-8 path").to_string();
        let n_s = n.to_string();
        let mut args: Vec<&str> = extra.to_vec();
        args.extend(["--bands", &n_s, "--png", &out_s, file]);
        let (ok, stdout, stderr) = pscat(&args);
        assert!(ok, "{tag}: --bands {n} failed: {stderr}");
        assert_eq!(
            stdout.matches("pscat: wrote").count(),
            1,
            "{tag}: one announcement: {stdout}"
        );
        let got = std::fs::read(&out).expect("banded png");
        assert!(
            got == want,
            "{tag}: --bands {n} differs from the plain render"
        );
    }
}

/// colorimage with a data procedure, rotated so the image's rows run
/// across the band seams at an angle. Unaware of bands.
const COLORIMAGE_ROTATED: &str = "%!PS
/W 64 def /H 48 def
/row W 3 mul string def /j 0 def
60 40 translate 9 rotate 220 160 scale
W H 8 [W 0 0 H neg 0 H]
{ 0 1 W 1 sub { /i exch def
    row i 3 mul i 4 mul 255 mod put
    row i 3 mul 1 add j 5 mul 255 mod put
    row i 3 mul 2 add i j add 7 mul 255 mod put } for
  /j j 1 add def row }
false 3 colorimage
";

/// An fbm2 field through a colorimage procedure that computes only the
/// image rows its band needs (one row of margin), and leaves the rest
/// unwritten. Run without bands it computes every row.
const FBM_AWARE: &str = "%!PS
/W 120 def /H 90 def /IX 20 def /IY 15 def /IW 240 def /IH 180 def
/rh IH H div def
/BandBBox where
  { pop /blo BandBBox 1 get rh sub def /bhi BandBBox 3 get rh add def }
  { /blo -1e9 def /bhi 1e9 def } ifelse
/row W 3 mul string def /j 0 def
/need { % j -> bool: does image row j (0 = top) reach the band?
  dup H exch sub rh mul IY add blo ge
  exch H exch sub 1 sub rh mul IY add bhi le and } def
IX IY translate IW IH scale
W H 8 [W 0 0 H neg 0 H]
{ j need {
    0 1 W 1 sub { /i exch def
      i 0.04 mul j 0.04 mul 5 0.5 2 fbm2 0.5 mul 0.5 add
      dup 0 lt { pop 0 } if dup 1 gt { pop 1 } if 255 mul cvi /v exch def
      row i 3 mul v put row i 3 mul 1 add 255 v sub put row i 3 mul 2 add v 2 idiv put
    } for } if
  /j j 1 add def row }
false 3 colorimage
";

/// Anti-aliased strokes, curves, text, alpha, a dash, a clip and a
/// gradient, all crossing the seams of every band count tested.
const STROKES: &str = "%!PS
7 srand
1 1 40 { pop
  rand 300 mod rand 220 mod moveto rand 300 mod rand 220 mod lineto
  rand 100 mod 100 div rand 100 mod 100 div rand 100 mod 100 div setrgbcolor
  rand 50 mod 10 div 0.3 add setlinewidth 1 setlinecap stroke } for
0.4 setalpha 0 0.3 0.9 setrgbcolor
150 110 70 0 360 arc fill
1 setalpha
[7 3] 0 setdash 2.5 setlinewidth 0.8 0.1 0.1 setrgbcolor
20 30 moveto 120 260 200 -40 290 200 curveto stroke
[] 0 setdash
/Helvetica findfont 31 scalefont setfont
gsave 30 120 translate 23 rotate 0 setgray 0 0 moveto (Bands!) show grestore
gsave 180 20 100 90 rectclip
<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [180 20 280 110]
   /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0.8 0] /C1 [0 0.5 0.4] /N 1 >> >> shfill
grestore
";

#[test]
fn colorimage_procedure_matches_byte_for_byte() {
    let dir = common::scratch_dir("bands-colorimage");
    let file = write_source(&dir, "colorimage.ps", COLORIMAGE_ROTATED);
    let file = file.to_str().expect("utf-8 path");
    assert_bands_match(
        "bands-colorimage-r",
        file,
        &["--page", "300x220", "--dpi", "100"],
        &[2, 3, 7, 8],
    );
}

#[test]
fn band_aware_fbm2_matches_byte_for_byte() {
    let dir = common::scratch_dir("bands-fbm");
    let file = write_source(&dir, "fbm.ps", FBM_AWARE);
    let file = file.to_str().expect("utf-8 path");
    assert_bands_match(
        "bands-fbm-r",
        file,
        &["--page", "280x210", "--dpi", "100"],
        &[2, 3, 7, 8],
    );
}

#[test]
fn strokes_and_anti_aliasing_across_seams_match_byte_for_byte() {
    let dir = common::scratch_dir("bands-strokes");
    let file = write_source(&dir, "strokes.ps", STROKES);
    let file = file.to_str().expect("utf-8 path");
    assert_bands_match(
        "bands-strokes-r",
        file,
        &["--page", "300x220", "--dpi", "100"],
        &[2, 3, 7, 8],
    );
    // Transparent background and the halftone screen go through the
    // stitch too.
    assert_bands_match(
        "bands-strokes-t",
        file,
        &["--page", "300x220", "--transparent"],
        &[3],
    );
    assert_bands_match(
        "bands-strokes-h",
        file,
        &["--page", "300x220", "--halftone"],
        &[5],
    );
}

#[test]
fn artkit_piece_matches_byte_for_byte() {
    assert_bands_match(
        "bands-artkit",
        "examples/fractals.ps",
        &["--dpi", "50"],
        &[2, 3, 8],
    );
}

#[test]
fn every_page_of_a_multi_page_program_is_banded() {
    let dir = common::scratch_dir("bands-pages");
    let file = write_source(
        &dir,
        "pages.ps",
        "%!PS\n0 0 1 setrgbcolor 10 10 80 60 rectfill showpage\n\
         1 0 0 setrgbcolor 5 5 moveto 95 75 lineto 3 setlinewidth stroke showpage\n",
    );
    let file = file.to_str().expect("utf-8 path");
    let plain = dir.join("plain.png");
    let banded = dir.join("banded.png");
    let (ok, _, e) = pscat(&["--page", "100x80", "--png", plain.to_str().unwrap(), file]);
    assert!(ok, "{e}");
    let (ok, out, e) = pscat(&[
        "--page",
        "100x80",
        "--bands",
        "3",
        "--png",
        banded.to_str().unwrap(),
        file,
    ]);
    assert!(ok, "{e}");
    assert_eq!(out.matches("pscat: wrote").count(), 2, "{out}");
    for page in ["001", "002"] {
        let p = std::fs::read(dir.join(format!("plain-{page}.png"))).expect("plain page");
        let b = std::fs::read(dir.join(format!("banded-{page}.png"))).expect("banded page");
        assert!(p == b, "page {page} differs");
    }
}

/// The control the byte-identity tests can't be: every band paints its own
/// grey, so the stitched page shows where each band's rows landed.
#[test]
fn each_band_lands_on_its_own_rows_top_down() {
    let dir = common::scratch_dir("bands-stripes");
    let png = dir.join("stripes.png");
    let n = 7u32;
    let (ok, _, e) = pscat(&[
        "--page",
        "40x101",
        "--dpi",
        "100",
        "--bands",
        &n.to_string(),
        "--png",
        png.to_str().unwrap(),
        "-e",
        "BandIndex 10 mul 255 div setgray clippath fill",
    ]);
    assert!(ok, "{e}");
    let pix = Pixmap::load_png(&png).expect("png");
    let h = pix.height();
    assert_eq!(h, 140, "101pt at 100dpi");
    for y in 0..h {
        // floor(i*h/n) <= y < floor((i+1)*h/n)
        let band = (0..n)
            .find(|&i| y < (u64::from(i + 1) * u64::from(h) / u64::from(n)) as u32)
            .expect("row in some band");
        let px = pix.pixel(pix.width() / 2, y).expect("pixel");
        assert_eq!(
            u32::from(px.red()),
            band * 10,
            "row {y} belongs to band {band}"
        );
    }
}

#[test]
fn band_names_are_defined_in_userdict() {
    let (ok, out, e) = pscat(&[
        "--page",
        "100x200",
        "--bands",
        "2",
        "--png",
        common::scratch_dir("bands-names")
            .join("n.png")
            .to_str()
            .unwrap(),
        "-e",
        "userdict /BandIndex get = BandCount = BandBBox ==",
    ]);
    assert!(ok, "{e}");
    // Only band 1 (the top) prints.
    assert_eq!(out.lines().next(), Some("0"), "{out}");
    assert!(out.contains("2\n[0 100 100 200]"), "{out}");
}

#[test]
fn an_error_still_writes_the_stitched_partial_page_and_fails() {
    let dir = common::scratch_dir("bands-error");
    let file = write_source(
        &dir,
        "err.ps",
        "%!PS\n0 0 1 setrgbcolor 10 10 60 60 rectfill 1 0 div\n",
    );
    let file = file.to_str().unwrap();
    let plain = dir.join("plain.png");
    let banded = dir.join("banded.png");
    let (ok, _, _) = pscat(&["--page", "80x80", "--png", plain.to_str().unwrap(), file]);
    assert!(!ok);
    let (ok, _, e) = pscat(&[
        "--page",
        "80x80",
        "--bands",
        "3",
        "--png",
        banded.to_str().unwrap(),
        file,
    ]);
    assert!(!ok, "an error fails the exit code");
    assert!(
        e.contains("band 1/3 failed") && e.contains("undefinedresult"),
        "{e}"
    );
    assert_eq!(
        e.matches("undefinedresult").count(),
        1,
        "reported once: {e}"
    );
    assert!(std::fs::read(plain).unwrap() == std::fs::read(banded).unwrap());
}

#[test]
fn deep_nesting_does_not_overflow_a_band_thread() {
    let deep = format!(
        "{}{} pop {}",
        "[".repeat(10_000),
        "]".repeat(10_000),
        "{".repeat(100_000)
    );
    let dir = common::scratch_dir("bands-deep");
    let file = write_source(&dir, "deep.ps", &deep);
    let (ok, _, e) = pscat(&[
        "--page",
        "20x20",
        "--bands",
        "2",
        "--png",
        dir.join("d.png").to_str().unwrap(),
        file.to_str().unwrap(),
    ]);
    assert!(!ok, "the open braces are a PostScript error");
    assert!(e.contains("band 1/2 failed"), "an error, not a crash: {e}");
}

#[test]
fn refuses_what_it_cannot_honour() {
    let png = common::scratch_dir("bands-refuse").join("r.png");
    let png = png.to_str().unwrap();
    for (args, want) in [
        (vec!["--bands", "2", "--svg", "x.svg", "-e", "0"], "--svg"),
        (vec!["--bands", "2", "--pdf", "x.pdf", "-e", "0"], "--pdf"),
        (
            vec!["--bands", "2", "--png", png, "--lint", "-e", "0"],
            "--lint",
        ),
        (vec!["--bands", "2", "--headless", "-e", "0"], "needs --png"),
        (
            vec![
                "--bands",
                "2",
                "--png",
                png,
                "--allow-write",
                ".",
                "-e",
                "0",
            ],
            "--allow-write",
        ),
        (
            vec!["--bands", "2", "--png", png, "--grid", "2x2", "-e", "0"],
            "--grid",
        ),
        (vec!["--bands", "0", "--png", png, "-e", "0"], "1..64"),
        (vec!["--bands", "65", "--png", png, "-e", "0"], "1..64"),
        (
            vec!["--bands", "9", "--page", "8x8", "--png", png, "-e", "0"],
            "pixel rows",
        ),
    ] {
        let (ok, _, e) = pscat(&args);
        assert!(!ok, "{args:?} must fail");
        assert!(e.contains(want), "{args:?}: {e}");
    }
}

/// A step budget that fits the program fits it under `--bands` too: the
/// band names are defined before the limit starts counting.
#[test]
fn step_limit_covers_the_program_not_the_band_names() {
    let png = common::scratch_dir("bands-steps").join("s.png");
    let (ok, _, e) = pscat(&[
        "--page",
        "20x20",
        "--max-steps",
        "3",
        "--bands",
        "2",
        "--png",
        png.to_str().unwrap(),
        "-e",
        "0 pop",
    ]);
    assert!(ok, "{e}");
}

/// `--confine-reads` (#763) binds every band, not just a plain render.
#[test]
fn read_roots_bind_every_band() {
    let dir = common::scratch_dir("bands-reads");
    let secret = write_source(&dir, "secret.ps", "/Secret 1 def\n");
    let png = dir.join("r.png");
    let prog = format!("({}) run", secret.display());
    let (ok, _, e) = pscat(&[
        "--page",
        "20x20",
        "--confine-reads",
        "--bands",
        "3",
        "--png",
        png.to_str().unwrap(),
        "-e",
        &prog,
    ]);
    assert!(!ok, "a confined band must not read outside the roots");
    assert!(
        e.contains("invalidfileaccess") || e.contains("undefinedfilename"),
        "{e}"
    );
    let (ok, _, e) = pscat(&[
        "--page",
        "20x20",
        "--bands",
        "3",
        "--png",
        png.to_str().unwrap(),
        "-e",
        &prog,
    ]);
    assert!(ok, "unconfined, the read works: {e}");
}
