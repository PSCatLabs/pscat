//! `pscat --bands N` (issue #736): a banded render is the plain `--png`
//! render, byte for byte, and the compositor puts each band's rows where
//! they belong.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tiny_skia::Pixmap;

mod common;
use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Run pscat with `args` (cwd: the crate root, so `(lib/...) run` resolves).
fn pscat(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output_deadline()
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
        // 140 rows / 20 = 7 bands, one per thread: the compositor, not the
        // default split, is under test here.
        "--band-rows",
        "20",
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
        "--band-rows",
        "100",
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
        "--band-rows",
        "27",
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
        (
            vec!["--band-rows", "4", "--png", png, "-e", "0"],
            "--band-rows needs --bands",
        ),
        (
            vec!["--bands", "2", "--band-rows", "0", "--png", png, "-e", "0"],
            "invalid --band-rows",
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
    // Backslashes in a Windows path are escapes in a PostScript string.
    let prog = format!(
        "({}) run",
        secret.display().to_string().replace('\\', "\\\\")
    );
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

/// Many more bands than threads (#844): the work queue hands out 140
/// one-row bands to 2 threads, more than `--bands` itself allows (64), and
/// every row still lands where it belongs.
#[test]
fn a_queue_of_many_small_bands_lands_every_row() {
    let png = common::scratch_dir("bands-queue").join("q.png");
    let (ok, _, e) = pscat(&[
        "--page",
        "40x101",
        "--dpi",
        "100",
        "--bands",
        "2",
        "--band-rows",
        "1",
        "--png",
        png.to_str().unwrap(),
        "-e",
        "BandCount 140 ne { stop } if BandIndex 255 div setgray clippath fill",
    ]);
    assert!(ok, "{e}");
    let pix = Pixmap::load_png(&png).expect("png");
    assert_eq!(pix.height(), 140);
    for y in 0..pix.height() {
        let px = pix.pixel(0, y).expect("pixel");
        assert_eq!(u32::from(px.red()), y, "row {y} is band {y}'s");
    }
}

/// The default split (#844) cuts the page into more bands than threads,
/// and the band names say so; one thread gets one band.
#[test]
fn band_names_describe_the_queued_band() {
    let png = common::scratch_dir("bands-default").join("d.png");
    let names = |threads: &str| {
        let (ok, out, e) = pscat(&[
            "--page",
            "100x200",
            "--bands",
            threads,
            "--png",
            png.to_str().unwrap(),
            "-e",
            "BandIndex = BandCount = BandBBox ==",
        ]);
        assert!(ok, "{e}");
        out
    };
    // 200 rows, 1 thread: one band, the whole page.
    assert!(
        names("1").starts_with("0\n1\n[0 0 100 200]"),
        "{}",
        names("1")
    );
    // 200 rows, 3 threads: 6 bands, the top one 33 rows tall.
    let out = names("3");
    assert!(out.starts_with("0\n6\n[0 167 100 200]"), "{out}");
}

#[test]
fn explicit_band_rows_match_byte_for_byte() {
    let dir = common::scratch_dir("bands-rows");
    let file = write_source(&dir, "fbm.ps", FBM_AWARE);
    let file = file.to_str().expect("utf-8 path");
    let page = ["--page", "280x210", "--dpi", "100"];
    let render = |out: &Path, more: &[&str]| {
        let out = out.to_str().expect("utf-8 path");
        let mut args = page.to_vec();
        args.extend(more);
        args.extend(["--png", out, file]);
        let (ok, _, e) = pscat(&args);
        assert!(ok, "{more:?}: {e}");
        std::fs::read(out).expect("png")
    };
    let want = render(&dir.join("plain.png"), &[]);
    for rows in ["1", "5", "37"] {
        let got = render(
            &dir.join(format!("rows{rows}.png")),
            &["--bands", "3", "--band-rows", rows],
        );
        assert!(
            got == want,
            "--band-rows {rows} differs from the plain render"
        );
    }
}

/// `--max-memory` is one budget for the whole banded render: a program
/// that fits it plainly fits it banded, and one that doesn't is stopped.
#[test]
fn max_memory_is_one_budget_for_all_bands() {
    let png = common::scratch_dir("bands-mem").join("m.png");
    let run = |prog: &str| {
        pscat(&[
            "--page",
            "200x200",
            "--max-memory",
            "40",
            "--bands",
            "4",
            "--png",
            png.to_str().unwrap(),
            "-e",
            prog,
        ])
    };
    let (ok, _, e) = run("/a 100000 array def 1 1 10 { pop 10000 string pop } for");
    assert!(ok, "a small program fits: {e}");
    let (ok, _, e) = run("/keep 100000 array def 0 1 99999 { keep exch 1000 array put } for");
    assert!(!ok, "a big one is stopped");
    assert!(e.contains("memory"), "{e}");
}

/// A limit stops the queue (#844): with 140 one-row bands on 2 threads, a
/// runaway program is stopped after one band's `--max-time` per thread, not
/// one per band (70 seconds here). The partial page is still written, the
/// run fails, and the bottom band, which would have run last, never did.
#[test]
fn a_limit_stops_the_queue() {
    let png = common::scratch_dir("bands-limit").join("l.png");
    let _ = std::fs::remove_file(&png);
    let (ok, _, e) = pscat(&[
        "--page",
        "40x101",
        "--dpi",
        "100",
        "--bands",
        "2",
        "--band-rows",
        "1",
        "--max-time",
        "1",
        "--png",
        png.to_str().unwrap(),
        "-e",
        "0 setgray clippath fill { } loop",
    ]);
    assert!(!ok, "a limit fails the run");
    assert!(e.contains("limitexceeded"), "{e}");
    let pix = Pixmap::load_png(&png).expect("the partial page is written");
    // The first bands ran (and painted black before the loop); the last
    // never started, so its row is the white background.
    assert_eq!(pix.pixel(0, 0).expect("pixel").red(), 0);
    assert_eq!(pix.pixel(0, 139).expect("pixel").red(), 255);
}

/// `forall` order over name keys follows the names' interned ids, and the
/// interner is per thread. Each band runs on a fresh thread (#844), so it
/// starts from the ids a plain run starts from, not the ones an earlier band
/// on the same worker left behind. Here the top band interns one extra name
/// before the dictionary is built; its own rows don't show the dictionary,
/// but on a reused thread the bottom band's `forall` order would shift.
/// (The first line interns the band names where the band preamble does, so
/// the plain run's ids line up with a band's.)
#[test]
fn a_band_does_not_inherit_an_earlier_bands_names() {
    let dir = common::scratch_dir("bands-names-ids");
    let file = write_source(
        &dir,
        "ids.ps",
        "%!PS
/BandIndex /BandCount /BandBBox pop pop pop
/BandIndex where { pop BandIndex 0 eq } { false } ifelse
  { (only_in_the_top_band) cvn pop } if
/d 40 dict def
0 1 39 { /k exch def d (key00) 5 string copy dup 3 k 10 idiv 48 add put
         dup 4 k 10 mod 48 add put cvn k put } for
/c 0 def
d { exch pop 255 div setgray c 3 mul 0 3 40 rectfill /c c 1 add def } forall
showpage
",
    );
    let file = file.to_str().expect("utf-8 path");
    let render = |name: &str, more: &[&str]| {
        let out = dir.join(name);
        let mut args = vec!["--page", "120x100"];
        args.extend(more);
        args.extend(["--png", out.to_str().unwrap(), file]);
        let (ok, _, e) = pscat(&args);
        assert!(ok, "{e}");
        std::fs::read(out).unwrap()
    };
    // One thread, two bands: the bottom band follows the top one.
    assert!(
        render("plain.png", &[]) == render("banded.png", &["--bands", "1", "--band-rows", "50"]),
        "the bottom band's forall order differs from a plain run's"
    );
}

/// Run `--bands 4 --page 100x100 -e program` in an isolated home (no font
/// packs installed, so a pack face is a stand-in with a note), returning
/// (success, stderr). A 100-pixel page at 72 dpi makes 8 bands, so a count
/// of one is a real check that bands above 0 are quiet.
fn bands_stderr(tag: &str, extra: &[&str], program: &str) -> (bool, String) {
    let home = common::scratch_dir(tag);
    let work = home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let png = home.join("s.png");
    let out = Command::new(BIN)
        .args(["--bands", "4", "--page", "100x100"])
        .args(extra)
        .args(["--png", png.to_str().unwrap(), "-e", program])
        .current_dir(&work)
        .env("PSCAT_HOME", &home)
        .env("HOME", home.join("user"))
        .env("XDG_DATA_HOME", home.join("user/data"))
        .env("PSCAT_NO_CHECKOUT", "1")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env_remove("APPDATA")
        .stdin(Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Only the top band writes stderr (#877): a `(%stderr)` line, which every
/// band used to print, comes out once.
#[test]
fn stderr_prints_once() {
    let (ok, err) = bands_stderr(
        "bands-stderr",
        &[],
        "(%stderr) (w) file (to-stderr\\n) writestring",
    );
    assert!(ok, "{err}");
    assert_eq!(err.matches("to-stderr").count(), 1, "{err}");
}

/// A stand-in font note is printed once, including one that only a lower
/// band reached (`BandIndex 2` here), and each note once.
#[test]
fn font_notes_are_deduplicated() {
    let (ok, err) = bands_stderr(
        "bands-font-notes",
        &[],
        "/Bangers findfont pop BandIndex 2 eq { /CourierPrime findfont pop } if",
    );
    assert!(ok, "{err}");
    assert_eq!(err.matches("/Bangers is in").count(), 1, "{err}");
    assert_eq!(err.matches("/CourierPrime is in").count(), 1, "{err}");
}

/// An uncaught error raised in one band above 0 only is still reported,
/// once, with its band, and fails the run.
#[test]
fn an_error_in_a_lower_band_is_reported_once() {
    let (ok, err) = bands_stderr(
        "bands-lower-error",
        &["--band-rows", "25"],
        "(%stderr) (w) file (to-stderr\\n) writestring BandIndex 2 eq { 1 0 div } if",
    );
    assert!(!ok, "the error fails the run: {err}");
    assert_eq!(err.matches("band 3/4 failed").count(), 1, "{err}");
    assert_eq!(err.matches("undefinedresult").count(), 1, "{err}");
    assert_eq!(err.matches("to-stderr").count(), 1, "{err}");
}

// --- --lint with --bands (#847) -------------------------------------------

/// The lint lines of a banded run's stderr.
fn lint_lines(err: &str) -> Vec<&str> {
    err.lines()
        .filter(|l| l.starts_with("pscat: lint: "))
        .collect()
}

/// How many `pscat: lint: [check]` findings of one check there are.
fn findings(err: &str, check: &str) -> usize {
    err.matches(&format!("pscat: lint: [{check}]")).count()
}

const PAINTS: &str = "0 0 1 setrgbcolor 0 0 100 100 rectfill";

/// A clean banded program: one note saying band 0 was linted, one verdict,
/// however many bands ran (8 here).
#[test]
fn lint_with_bands_prints_once_and_says_which_band() {
    let (ok, err) = bands_stderr("bands-lint-clean", &["--lint"], PAINTS);
    assert!(ok, "{err}");
    assert_eq!(err.matches("pscat: lint: clean").count(), 1, "{err}");
    let notes: Vec<_> = err.lines().filter(|l| l.contains("lint: note:")).collect();
    assert_eq!(notes.len(), 1, "{err}");
    assert!(notes[0].contains("band 1 of 8"), "{}", notes[0]);
    assert_eq!(lint_lines(&err).len(), 2, "{err}");
}

/// A program that paints nothing is one `blank-page`, not one per band.
#[test]
fn lint_with_bands_reports_a_blank_page_once() {
    let (ok, err) = bands_stderr("bands-lint-blank", &["--lint"], "1 1 add pop");
    assert!(ok, "advisory lint does not fail the run: {err}");
    assert_eq!(findings(&err, "blank-page"), 1, "{err}");
    assert!(!err.contains("lint: clean"), "{err}");
}

/// A band-aware program paints nothing in the top band, so band 0's own
/// canvas is empty; the page is not blank, and lint must not say so.
#[test]
fn lint_with_bands_judges_blankness_on_the_stitched_page() {
    let (ok, err) = bands_stderr(
        "bands-lint-top-empty",
        &["--lint"],
        &format!("BandIndex 0 gt {{ {PAINTS} }} if"),
    );
    assert!(ok, "{err}");
    assert_eq!(findings(&err, "blank-page"), 0, "{err}");
    assert!(err.contains("pscat: lint: clean"), "{err}");
}

/// The behaviour checks run on band 0 once: an unbalanced gsave, a leaked
/// operand and a wrong `%%Pages:` count (two declared, one shown) each come
/// out once. Plain `--lint` stays advisory (exit 0); `--lint-strict` fails
/// the run, and its footer counts the findings, not the note.
#[test]
fn lint_with_bands_reports_band_zero_findings_once() {
    let program = format!("%%Pages: 2\n{PAINTS} gsave 7 showpage");
    let (ok, err) = bands_stderr("bands-lint-findings", &["--lint"], &program);
    assert!(ok, "{err}");
    for check in ["page-count", "gsave-imbalance", "stack-leak"] {
        assert_eq!(findings(&err, check), 1, "{check}: {err}");
    }
    let (ok, err) = bands_stderr("bands-lint-strict", &["--lint-strict"], &program);
    assert!(!ok, "--lint-strict fails on findings: {err}");
    assert!(err.contains("3 finding(s)"), "{err}");
}

/// What the note admits: a program that leaks only in a band other than 0
/// is not caught.
#[test]
fn lint_with_bands_does_not_see_a_leak_in_another_band() {
    let (ok, err) = bands_stderr(
        "bands-lint-other-band",
        &["--lint", "--band-rows", "25"],
        &format!("{PAINTS} BandIndex 2 eq {{ 1 2 3 }} if"),
    );
    assert!(ok, "{err}");
    assert_eq!(findings(&err, "stack-leak"), 0, "{err}");
}

/// An error in a lower band only is a `run-error`, never "clean", and the
/// report it points at is printed before lint.
#[test]
fn lint_with_bands_is_not_clean_after_an_error_in_any_band() {
    let (ok, err) = bands_stderr(
        "bands-lint-error",
        &["--lint", "--band-rows", "25"],
        &format!("{PAINTS} BandIndex 2 eq {{ 1 0 div }} if"),
    );
    assert!(!ok, "{err}");
    assert_eq!(findings(&err, "run-error"), 1, "{err}");
    assert!(!err.contains("lint: clean"), "{err}");
    let report = err.find("band 3/4 failed").expect("the band's report");
    assert!(report < err.find("[run-error]").unwrap(), "{err}");
}
