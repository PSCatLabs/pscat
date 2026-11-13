//! Encoded number strings in `rectfill`, `rectstroke` and `rectclip`
//! (PLRM 3.14.5), and the homogeneous number array as a binary token
//! (token type 149, PLRM 3.14.2); issue #1192.
//!
//! The case list is plain PostScript that runs in pscat and in Ghostscript;
//! its report is pinned below from Ghostscript 10.07.1, so pscat is checked
//! against it with or without gs, and `ghostscript_prints_the_pinned_report`
//! checks the pin itself. Error lines carry no operand stack, since gs
//! leaves an erroring operator's operands and pscat consumes them by design.

use std::process::Command;

use pscat::{Interp, PsError};

mod common;
use common::CommandDeadline as _;

/// `case` runs a body on a fresh path and clip and prints `name => ok` or
/// `name => errorname`; `bb` prints the clip's bounding box.
const PRELUDE: &str = r#"
/r2 { 100 mul round 100 div } def
/out { dup type /stringtype eq { print } { 64 string cvs print } ifelse } def
/bb { clippath pathbbox 4 { r2 out ( ) print } repeat } def
/case { exch out ( => ) print initclip newpath stopped
  { $error /errorname get out }
  { (ok) print } ifelse (\n) print clear } def
"#;

/// One case per line.
const CASES: &str = r#"
(array) { [10 20 30 40] rectclip bb } case
(array two rects) { [10 20 30 40 100 100 10 10] rectclip bb } case
(array empty) { [] rectclip } case
(array reals) { [10.5 20.25 30 40] rectclip bb } case
(packed array) { { 10 20 30 40 } rectclip bb } case
(ns 16 be) { <95 20 0004 000a 0014 001e 0028> rectclip bb } case
(ns 16 be frac) { <95 21 0004 0014 0028 003c 0050> rectclip bb } case
(ns 16 le) { <95 a0 0400 0a00 1400 1e00 2800> rectclip bb } case
(ns 16 le frac) { <95 a1 0400 1400 2800 3c00 5000> rectclip bb } case
(ns 32 be) { <95 00 0004 0000000a 00000014 0000001e 00000028> rectclip bb } case
(ns 32 be frac) { <95 10 0004 000a8000 00140000 001e0000 00280000> rectclip bb } case
(ns 32 le) { <95 80 0400 0a000000 14000000 1e000000 28000000> rectclip bb } case
(ns 32 le frac) { <95 90 0400 00800a00 00001400 00001e00 00002800> rectclip bb } case
(ns real be) { <95 30 0004 41200000 41a00000 41f00000 42200000> rectclip bb } case
(ns real le) { <95 b0 0400 00002041 0000a041 0000f041 00002042> rectclip bb } case
(ns native be count) { <95 31 0004 00002041 0000a041 0000f041 00002042> rectclip bb } case
(ns native le count) { <95 b1 0400 00002041 0000a041 0000f041 00002042> rectclip bb } case
(ns negative size) { <95 20 0004 0064 0064 ffe2 ffd8> rectclip bb } case
(ns two rects) { <95 20 0008 000a 0014 001e 0028 0064 0064 000a 000a> rectclip bb } case
(ns empty) { <95 20 0000> rectclip } case
(ns stray byte) { <95 20 0004 000a 0014 001e 0028 00> rectclip bb } case
(ns 3 values) { <95 20 0003 0001 0002 0003> rectclip } case
(ns 5 values) { <95 20 0005 0001 0002 0003 0004 0005> rectclip } case
(ns 6 values) { <95 20 0006 0001 0002 0003 0004 0005 0006> rectclip } case
(array 3 values) { [1 2 3] rectclip } case
(array 5 values) { [1 2 3 4 5] rectclip } case
(array 6 values) { [1 2 3 4 5 6] rectclip } case
(array non-number) { [1 2 /x 4] rectclip } case
(array non-number tail) { [1 2 3 4 /x] rectclip } case
(array non-number 3) { [1 2 /x] rectclip } case
(ns short header) { <95 20 00> rectclip } case
(ns bad first byte) { <94 20 0004 0001 0002 0003 0004> rectclip } case
(ns bad representation) { <95 32 0004 0001 0002 0003 0004> rectclip } case
(ns count long) { <95 20 0004 0001 0002 0003> rectclip } case
(ns count short) { <95 20 0003 0001 0002 0003 0004> rectclip } case
(ns whole extra value) { <95 20 0004 0001 0002 0003 0004 0005> rectclip } case
(ns infinity) { <95 30 0004 7f800000 00000000 00000000 00000000> rectclip } case
(ns nan) { <95 30 0004 7fc00000 00000000 00000000 00000000> rectclip } case
(ns count 3 before infinity) { <95 30 0003 7f800000 00000000 00000000> rectclip } case
(ns count 5 before infinity) { <95 30 0005 7f800000 00000000 00000000 00000000 00000000> rectclip } case
(ns bad header before count) { <95 32 0003 0001 0002 0003> rectclip } case
(empty string) { () rectclip } case
(plain string) { (abcd) rectclip } case
(name) { 1 2 3 /x rectclip } case
(three numbers) { 1 2 3 rectclip } case
(fill ns) { <95 20 0004 0001 0002 0003 0004> rectfill } case
(fill ns real) { <95 30 0004 3f800000 40000000 40400000 40800000> rectfill } case
(fill ns 3 values) { <95 20 0003 0001 0002 0003> rectfill } case
(fill array 3 values) { [1 2 3] rectfill } case
(fill array non-number) { [1 2 /x 4] rectfill } case
(fill ns bad representation) { <95 32 0004 0001 0002 0003 0004> rectfill } case
(fill ns count long) { <95 20 0004 0001 0002 0003> rectfill } case
(fill ns infinity) { <95 30 0004 7f800000 00000000 00000000 00000000> rectfill } case
(fill short header) { <95 20 00> rectfill } case
(fill plain string) { (abcd) rectfill } case
(stroke ns) { <95 20 0004 0001 0002 0003 0004> rectstroke } case
(stroke ns le) { <95 a0 0400 0100 0200 0300 0400> rectstroke } case
(stroke ns matrix) { <95 20 0004 0001 0002 0003 0004> [1 0 0 1 0 0] rectstroke } case
(stroke ns 3 values) { <95 20 0003 0001 0002 0003> rectstroke } case
(stroke ns 6 values) { <95 20 0006 0001 0002 0003 0004 0005 0006> rectstroke } case
(stroke array 3 values) { [1 2 3] rectstroke } case
(stroke array 8 values matrix) { [1 2 3 4 5 6 7 8] [1 0 0 1 0 0] rectstroke } case
(stroke array non-number) { [1 2 /x 4] rectstroke } case
(stroke ns bad representation) { <95 32 0004 0001 0002 0003 0004> rectstroke } case
(stroke ns bad header with matrix) { <94 20 0004 0001 0002 0003 0004> [1 0 0 1 0 0] rectstroke } case
(stroke ns infinity) { <95 30 0004 7f800000 00000000 00000000 00000000> rectstroke } case
(stroke ns infinity with matrix) { <95 30 0004 7f800000 00000000 00000000 00000000> [1 0 0 1 0 0] rectstroke } case
(stroke ns under a matrix-sized array) { [1 0 0 1 0 0] <95 20 0004 0001 0002 0003 0004> rectstroke } case
(stroke plain string with matrix) { (abcd) [1 0 0 1 0 0] rectstroke } case
(stroke four numbers with matrix) { 1 2 3 4 [1 0 0 1 0 0] rectstroke } case
"#;

/// What Ghostscript 10.07.1 prints for `CASES`, one line per case.
const EXPECTED: &str = r#"
array => 60.0 40.0 20.0 10.0 ok
array two rects => 110.0 110.0 20.0 10.0 ok
array empty => ok
array reals => 60.25 40.5 20.25 10.5 ok
packed array => 60.0 40.0 20.0 10.0 ok
ns 16 be => 60.0 40.0 20.0 10.0 ok
ns 16 be frac => 60.0 40.0 20.0 10.0 ok
ns 16 le => 60.0 40.0 20.0 10.0 ok
ns 16 le frac => 60.0 40.0 20.0 10.0 ok
ns 32 be => 60.0 40.0 20.0 10.0 ok
ns 32 be frac => 60.0 40.5 20.0 10.5 ok
ns 32 le => 60.0 40.0 20.0 10.0 ok
ns 32 le frac => 60.0 40.5 20.0 10.5 ok
ns real be => 60.0 40.0 20.0 10.0 ok
ns real le => 60.0 40.0 20.0 10.0 ok
ns native be count => 60.0 40.0 20.0 10.0 ok
ns native le count => 60.0 40.0 20.0 10.0 ok
ns negative size => 100.0 100.0 60.0 70.0 ok
ns two rects => 110.0 110.0 20.0 10.0 ok
ns empty => ok
ns stray byte => 60.0 40.0 20.0 10.0 ok
ns 3 values => typecheck
ns 5 values => typecheck
ns 6 values => typecheck
array 3 values => typecheck
array 5 values => typecheck
array 6 values => typecheck
array non-number => typecheck
array non-number tail => typecheck
array non-number 3 => typecheck
ns short header => typecheck
ns bad first byte => typecheck
ns bad representation => rangecheck
ns count long => rangecheck
ns count short => rangecheck
ns whole extra value => rangecheck
ns infinity => undefinedresult
ns nan => undefinedresult
ns count 3 before infinity => typecheck
ns count 5 before infinity => typecheck
ns bad header before count => rangecheck
empty string => typecheck
plain string => typecheck
name => typecheck
three numbers => stackunderflow
fill ns => ok
fill ns real => ok
fill ns 3 values => typecheck
fill array 3 values => typecheck
fill array non-number => typecheck
fill ns bad representation => rangecheck
fill ns count long => rangecheck
fill ns infinity => undefinedresult
fill short header => typecheck
fill plain string => typecheck
stroke ns => ok
stroke ns le => ok
stroke ns matrix => ok
stroke ns 3 values => typecheck
stroke ns 6 values => typecheck
stroke array 3 values => typecheck
stroke array 8 values matrix => ok
stroke array non-number => typecheck
stroke ns bad representation => rangecheck
stroke ns bad header with matrix => typecheck
stroke ns infinity => undefinedresult
stroke ns infinity with matrix => undefinedresult
stroke ns under a matrix-sized array => ok
stroke plain string with matrix => typecheck
stroke four numbers with matrix => ok
"#;

/// The same for the scanner: `string tcase` scans the first token of a
/// string and prints the array it yields (`i` an integer, `r` a real), the
/// string left over, or the error.
const TOKEN_PRELUDE: &str = r#"
/r2 { 100 mul round 100 div } def
/out { dup type /stringtype eq { print } { 64 string cvs print } ifelse } def
/tcase { exch out ( => ) print { token } stopped
  { $error /errorname get out }
  { { (array ) print dup xcheck out ( ) print
      { dup type /integertype eq { (i ) print out } { (r ) print r2 out } ifelse ( ) print } forall
      ( rest ) print dup length out ( ) print { out ( ) print } forall }
    { (none) print } ifelse }
  ifelse (\n) print clear } def
"#;

const TOKEN_CASES: &str = r#"
(16 be) <95 20 0003 0001 0002 0003> tcase
(16 be frac) <95 21 0003 0002 0004 0007> tcase
(16 le) <95 a0 0300 0100 0200 0300> tcase
(16 le frac) <95 a1 0300 0200 0400 0700> tcase
(16 signed) <95 20 0003 ffff fffe 0003> tcase
(32 be) <95 00 0002 00018000 ffff0000> tcase
(32 be frac) <95 10 0002 00018000 ffff0000> tcase
(32 le) <95 80 0200 00800100 000000ff> tcase
(32 le frac) <95 90 0200 00800100 0000ffff> tcase
(real be) <95 30 0002 3fc00000 40000000> tcase
(real le) <95 b0 0200 0000c03f 00000040> tcase
(native be count) <95 31 0002 0000c03f 00000040> tcase
(native le count) <95 b1 0200 0000c03f 00000040> tcase
(empty) <95 20 0000> tcase
(rest follows) <95 20 0002 0001 0002 41 42> tcase
(stray byte stays) <95 20 0002 0001 0002 00> tcase
(whole extra value stays) <95 20 0002 0001 0002 0003> tcase
(leading white space) <20 0a 95 20 0001 0007> tcase
(truncated values) <95 20 0003 0001 0002> tcase
(truncated one value) <95 20 0003 0001> tcase
(truncated header) <95 20 00> tcase
(header only) <95> tcase
(bad representation) <95 32 0001 0001> tcase
(bad representation 2) <95 c8 0100 0001> tcase
(infinity) <95 30 0001 7f800000> tcase
(nan) <95 30 0001 7fc00000> tcase
(negative infinity) <95 b0 0100 000080ff> tcase
"#;

/// What Ghostscript 10.07.1 prints for `TOKEN_CASES`.
const TOKEN_EXPECTED: &str = r#"
16 be => array false i 1 i 2 i 3  rest 0 
16 be frac => array false r 1.0 r 2.0 r 3.5  rest 0 
16 le => array false i 1 i 2 i 3  rest 0 
16 le frac => array false r 1.0 r 2.0 r 3.5  rest 0 
16 signed => array false i -1 i -2 i 3  rest 0 
32 be => array false i 98304 i -65536  rest 0 
32 be frac => array false r 1.5 r -1.0  rest 0 
32 le => array false i 98304 i -16777216  rest 0 
32 le frac => array false r 1.5 r -1.0  rest 0 
real be => array false r 1.5 r 2.0  rest 0 
real le => array false r 1.5 r 2.0  rest 0 
native be count => array false r 1.5 r 2.0  rest 0 
native le count => array false r 1.5 r 2.0  rest 0 
empty => array false  rest 0 
rest follows => array false i 1 i 2  rest 2 65 66 
stray byte stays => array false i 1 i 2  rest 1 0 
whole extra value stays => array false i 1 i 2  rest 2 0 3 
leading white space => array false i 7  rest 0 
truncated values => syntaxerror
truncated one value => syntaxerror
truncated header => syntaxerror
header only => syntaxerror
bad representation => syntaxerror
bad representation 2 => syntaxerror
infinity => undefinedresult
nan => undefinedresult
negative infinity => undefinedresult
"#;

fn run_pscat(prelude: &str, body: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&format!("{prelude}{body}"))
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

fn run_gs(tag: &str, prelude: &str, body: &str) -> String {
    let dir = common::scratch_dir(tag);
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, format!("{prelude}{body}")).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Compare a report with its pin line by line, naming the case.
fn assert_matches_pin(who: &str, cases: &'static str, pin: &'static str, report: &str) {
    let lines =
        |s: &'static str| -> Vec<&'static str> { s.lines().filter(|l| !l.is_empty()).collect() };
    let got: Vec<&str> = report.lines().collect();
    let (want, cases) = (lines(pin), lines(cases));
    assert_eq!(cases.len(), want.len(), "one pinned line per case");
    assert_eq!(got.len(), want.len(), "{who} printed:\n{report}");
    for ((case, g), w) in cases.iter().zip(&got).zip(&want) {
        assert_eq!(
            g.trim_end(),
            w.trim_end(),
            "{who}, case {case}\n  got  {g}\n  want {w}"
        );
    }
}

#[test]
fn pscat_prints_the_pinned_rect_report() {
    assert_matches_pin("pscat", CASES, EXPECTED, &run_pscat(PRELUDE, CASES));
}

#[test]
fn ghostscript_prints_the_pinned_rect_report() {
    if !common::gs_available() {
        return;
    }
    let report = run_gs("numarray_rect_gs", PRELUDE, CASES);
    assert_matches_pin("gs", CASES, EXPECTED, &report);
}

#[test]
fn pscat_prints_the_pinned_token_report() {
    let report = run_pscat(TOKEN_PRELUDE, TOKEN_CASES);
    assert_matches_pin("pscat", TOKEN_CASES, TOKEN_EXPECTED, &report);
}

#[test]
fn ghostscript_prints_the_pinned_token_report() {
    if !common::gs_available() {
        return;
    }
    let report = run_gs("numarray_token_gs", TOKEN_PRELUDE, TOKEN_CASES);
    assert_matches_pin("gs", TOKEN_CASES, TOKEN_EXPECTED, &report);
}

// --- rendering ---------------------------------------------------------------

/// The rect operators with encoded strings in two byte orders and widths
/// (and a real one): filled, stroked with and without a matrix, and used
/// to clip a fill.
const SCENE: &str = "
    0.2 0.4 0.9 setrgbcolor
    <95 20 0008 000a 000a 0028 001e 0050 0050 001e 0028> rectfill
    0.9 0.2 0.1 setrgbcolor 4 setlinewidth
    <95 a0 0400 0a00 6400 5000 3200> rectstroke
    0 0.5 0 setrgbcolor 2 setlinewidth
    <95 30 0004 42a00000 41a00000 42200000 41f00000> [2 0 0 2 0 0] rectstroke
    gsave
    <95 00 0004 0000008c 00000014 0000001e 00000050> rectclip
    0.5 0.2 0.6 setrgbcolor 0 0 200 200 rectfill
    grestore
";

/// The same scene with plain arrays.
const SCENE_ARRAYS: &str = "
    0.2 0.4 0.9 setrgbcolor
    [10 10 40 30 80 80 30 40] rectfill
    0.9 0.2 0.1 setrgbcolor 4 setlinewidth
    [10 100 80 50] rectstroke
    0 0.5 0 setrgbcolor 2 setlinewidth
    [80 20 40 30] [2 0 0 2 0 0] rectstroke
    gsave
    [140 20 30 80] rectclip
    0.5 0.2 0.6 setrgbcolor 0 0 200 200 rectfill
    grestore
";

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(200, 200).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

/// Pixels whose colours differ by more than `over` (summed over R, G, B).
fn differing(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap, over: u32) -> usize {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(a, b)| {
            let d = |x: u8, y: u8| u32::from(x.abs_diff(y));
            d(a.red(), b.red()) + d(a.green(), b.green()) + d(a.blue(), b.blue()) > over
        })
        .count()
}

fn ink(pm: &tiny_skia::Pixmap) -> usize {
    pm.pixels().iter().filter(|p| p.green() < 200).count()
}

#[test]
fn encoded_strings_draw_what_the_arrays_draw() {
    let strings = render(SCENE);
    let arrays = render(SCENE_ARRAYS);
    assert!(ink(&strings.gfx().pixmap) > 3000, "the scene is empty");
    assert_eq!(differing(&strings.gfx().pixmap, &arrays.gfx().pixmap, 0), 0);
}

#[test]
fn ghostscript_draws_the_same_rects() {
    if !common::gs_available() {
        return;
    }
    let ours = render(SCENE);
    let dir = common::scratch_dir("numarray_gs_png");
    let (src, png) = (dir.join("scene.ps"), dir.join("out.png"));
    std::fs::write(&src, format!("{SCENE} showpage")).expect("write");
    let out = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dNOSAFER",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            "-g200x200",
            "-r72",
        ])
        .arg(format!("-o{}", png.display()))
        .arg(&src)
        .output_deadline()
        .expect("run gs");
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let theirs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_dir_all(&dir);
    let n = differing(&theirs, &ours.gfx().pixmap, 600);
    assert!(n < 40, "{n} pixels differ from gs");
}

#[test]
fn a_failed_rect_paints_and_clips_nothing() {
    for src in [
        "<95 20 0003 0001 0002 0003> rectfill",
        "<95 30 0004 7f800000 00000000 00000000 00000000> rectfill",
        "<95 32 0004 0001 0002 0003 0004> rectstroke",
        "[0 0 100 100 /x] rectfill",
    ] {
        let it = render(&format!("0 setgray {{ {src} }} stopped pop"));
        assert_eq!(ink(&it.gfx().pixmap), 0, "{src} painted");
    }
    let it =
        render("{ <95 20 0003 0001 0002 0003> rectclip } stopped pop 0 setgray 0 0 50 50 rectfill");
    assert!(ink(&it.gfx().pixmap) > 1000, "a failed rectclip clipped");
}

// --- the scanner: token 149 ------------------------------------------------

/// The operand stack after running program text with raw bytes in it.
fn stack_of(src: &[u8]) -> Vec<String> {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn error_of(src: &[u8]) -> PsError {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.run_source(src).expect_err("should fail")
}

/// A homogeneous number array token in the program itself (not scanned
/// from a string): both byte orders, integers and reals, and it is a
/// literal array that is pushed, not run.
#[test]
fn token_149_in_a_program_pushes_a_literal_array() {
    let mut src = vec![0x95, 0x20, 0, 3, 0, 1, 0, 2, 0, 3, b' '];
    src.extend_from_slice(&[0x95, 0xa0, 3, 0, 1, 0, 2, 0, 3, 0, b' ']);
    src.extend_from_slice(&[0x95, 0x30, 0, 2, 0x3f, 0xc0, 0, 0, 0x40, 0, 0, 0, b' ']);
    src.extend_from_slice(&[0x95, 0xb0, 2, 0, 0, 0, 0xc0, 0x3f, 0, 0, 0, 0x40]);
    src.extend_from_slice(b" 3 index xcheck");
    assert_eq!(
        stack_of(&src),
        ["[1 2 3]", "[1 2 3]", "[1.5 2.0]", "[1.5 2.0]", "false"]
    );
}

/// Fixed point with no fraction bits scans as integers, any other as reals,
/// whichever the width.
#[test]
fn token_149_integers_and_reals() {
    let mut src = vec![0x95, 0x00, 0, 2, 0, 1, 0x80, 0, 0xff, 0xff, 0, 0];
    src.extend_from_slice(&[0x95, 0x10, 0, 2, 0, 1, 0x80, 0, 0xff, 0xff, 0, 0]);
    src.extend_from_slice(&[0x95, 0x21, 0, 2, 0, 2, 0, 5]);
    assert_eq!(
        stack_of(&src),
        ["[98304 -65536]", "[1.5 -1.0]", "[1.0 2.5]"]
    );
}

/// The token stops after its numbers: the next byte is the next token.
#[test]
fn token_149_ends_where_its_numbers_end() {
    let mut src = vec![0x95, 0x20, 0, 2, 0, 1, 0, 2];
    src.extend_from_slice(b"7 8");
    assert_eq!(stack_of(&src), ["[1 2]", "7", "8"]);
}

/// Inside a procedure body it is one literal array element.
#[test]
fn token_149_inside_a_procedure() {
    let mut src = b"{ ".to_vec();
    src.extend_from_slice(&[0x95, 0x20, 0, 2, 0, 1, 0, 2]);
    src.extend_from_slice(b" } dup length exch 0 get");
    assert_eq!(stack_of(&src), ["1", "[1 2]"]);
}

/// `0 setobjectformat` turns binary tokens off: 149 is an ordinary name
/// character again, like 128 to 131.
#[test]
fn token_149_is_a_name_character_with_format_zero() {
    let mut src = b"0 setobjectformat /a".to_vec();
    src.push(0x95);
    src.extend_from_slice(b" 7 def a");
    src.push(0x95);
    assert_eq!(stack_of(&src), ["7"]);
}

#[test]
fn bad_token_149_errors_like_ghostscript() {
    // Truncated, short header, undefined representation: syntaxerror.
    for bytes in [
        &[0x95, 0x20, 0, 3, 0, 1, 0, 2][..],
        &[0x95, 0x20, 0, 3, 0, 1],
        &[0x95, 0x20, 0],
        &[0x95],
        &[0x95, 0x32, 0, 1, 0, 1],
        &[0x95, 0xc8, 1, 0, 0, 1],
    ] {
        assert!(
            matches!(error_of(bytes), PsError::Syntax(_)),
            "{bytes:02x?}: {:?}",
            error_of(bytes)
        );
    }
    // A non-finite real: undefinedresult.
    for bytes in [
        &[0x95, 0x30, 0, 1, 0x7f, 0x80, 0, 0][..],
        &[0x95, 0x30, 0, 1, 0x7f, 0xc0, 0, 0],
        &[0x95, 0xb0, 1, 0, 0, 0, 0x80, 0xff],
    ] {
        assert_eq!(error_of(bytes), PsError::UndefinedResult, "{bytes:02x?}");
    }
}
