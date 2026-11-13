//! `xshow`, `yshow`, `xyshow` and `cshow` (Level 2, PLRM 3rd ed. ch. 8;
//! issue #1180), and the encoded number strings the first three take
//! (PLRM 3.14.5).
//!
//! The case list is plain PostScript that runs in pscat and in Ghostscript;
//! its report is pinned below from Ghostscript 10.07.1, so pscat is checked
//! against it with or without gs, and `ghostscript_prints_the_pinned_report`
//! checks the pin itself. The glyphs come from a Type 3 font of triangles
//! (`BoxFont`), so widths and shapes are identical in both interpreters:
//! Liberation and gs's URW faces agree on widths only to a few thousandths.
//! Error lines carry no operand stack, since gs leaves an erroring
//! operator's operands and pscat consumes them by design.

use std::process::Command;

use pscat::{Interp, SvgText, Value};

mod common;
use common::CommandDeadline as _;

/// `case` runs a body on a fresh path with `BoxFont` at 40 pt and prints
/// `name => ok cp x y` (or `nocp`) or `name => errorname`, numbers rounded
/// to 0.01. `cs` prints a cshow proc's `code wx wy`, top first.
const PRELUDE: &str = r#"
/r2 { 100 mul round 100 div } def
/out { dup type /stringtype eq { print } { 64 string cvs print } ifelse } def
/num { dup type /realtype eq { r2 } if out ( ) print } def
/BoxFont << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0] /FontBBox [0 0 800 800]
  /Encoding StandardEncoding
  /BuildChar { exch pop dup 3 mod 100 mul 500 add exch 5 mod 100 mul 300 add
    1 index 0 setcharwidth 50 0 moveto exch 100 sub 0 rlineto 0 exch rlineto closepath fill } bind
>> definefont pop
/box { /BoxFont findfont 40 scalefont setfont } def
/case { exch out ( => ) print newpath box stopped
  { $error /errorname get out }
  { (ok) print { currentpoint } stopped { ( nocp) print } { ( cp ) print exch num num } ifelse }
  ifelse (\n) print clear } def
/cs { (cs ) print 3 { num } repeat } def
"#;

/// One case per line.
const CASES: &str = r#"
(xshow) { 20 30 moveto (abc) [10 20 30] xshow } case
(yshow) { 20 30 moveto (abc) [10 20 30] yshow } case
(xyshow) { 20 30 moveto (abc) [10 1 20 2 30 3] xyshow } case
(xyshow scaled) { 2 3 scale 10 20 moveto (abc) [1 2 3 4 5 6] xyshow } case
(xyshow rotated) { 30 rotate 0 0 moveto (ab) [10 0 0 10] xyshow } case
(xshow reals) { 0 0 moveto (ab) [1.5 2.25] xshow } case
(xshow negative) { 50 50 moveto (ab) [-10 -20.5] xshow } case
(extra numbers) { 0 0 moveto (ab) [1 2 3 4 5] xshow } case
(packed array) { 0 0 moveto (ab) { 1 2 } xshow } case
(empty) { 5 6 moveto () [] xshow } case
(empty numstring) { 5 6 moveto () <95 20 0000> xshow } case
(ns 16 be) { 0 0 moveto (abc) <95 20 0003 0001 0002 0003> xshow } case
(ns 16 be frac) { 0 0 moveto (abc) <95 21 0003 0002 0004 0007> xshow } case
(ns 16 le) { 0 0 moveto (abc) <95 a0 0300 0100 0200 0300> yshow } case
(ns 32 be) { 0 0 moveto (abc) <95 00 0003 00000001 00000002 00000003> xshow } case
(ns 32 be frac) { 0 0 moveto (abc) <95 10 0003 00018000 00020000 00030000> xshow } case
(ns 32 le) { 0 0 moveto (abc) <95 80 0300 01000000 02000000 03000000> xshow } case
(ns real be) { 0 0 moveto (abc) <95 30 0003 3fc00000 40000000 40400000> xshow } case
(ns real le) { 0 0 moveto (abc) <95 b0 0300 0000c03f 00000040 00004040> xshow } case
(ns native le) { 0 0 moveto (abc) <95 b1 0300 0000c03f 00000040 00004040> xshow } case
(ns signed) { 0 0 moveto (abc) <95 20 0003 ffff fffe 0003> xshow } case
(ns xyshow) { 0 0 moveto (ab) <95 20 0004 0001 0002 0003 0004> xyshow } case
(ns stray byte) { 0 0 moveto (abc) <95 20 0003 0001 0002 0003 00> xshow } case
(too few xshow) { 0 0 moveto (abc) [1 2] xshow } case
(too few yshow) { 0 0 moveto (abc) [1 2] yshow } case
(too few xyshow) { 0 0 moveto (abc) [1 2 3 4 5] xyshow } case
(non-number) { 0 0 moveto (abc) [1 /x 3] xshow } case
(non-number yshow) { 0 0 moveto (abc) [1 /x 3] yshow } case
(non-number xyshow) { 0 0 moveto (abc) [1 2 /x 4 5 6] xyshow } case
(unused non-number) { 0 0 moveto (ab) [1 2 /x] xshow } case
(numarray not array) { 0 0 moveto (ab) 5 xshow } case
(text not string) { 0 0 moveto 5 [1 2] xshow } case
(name numarray) { 0 0 moveto (ab) /x xshow } case
(no current point) { (ab) [1 2] xshow } case
(rangecheck before nocurrentpoint) { (abc) [1] xshow } case
(nocurrentpoint before typecheck) { (abc) [1 /x 2] xshow } case
(ns count short) { 0 0 moveto (abc) <95 20 0004 0001 0002 0003> xshow } case
(ns count long) { 0 0 moveto (abc) <95 20 0003 0001 0002 0003 0004> xshow } case
(ns too few for text) { 0 0 moveto (abc) <95 20 0002 0001 0002> xshow } case
(ns bad representation) { 0 0 moveto (abc) <95 32 0003 0001 0002 0003> xshow } case
(ns bad header) { 0 0 moveto (abc) <94 20 0003 0001 0002 0003> xshow } case
(ns short header) { 0 0 moveto (abc) <95 20 00> xshow } case
(ns empty) { 0 0 moveto () () xshow } case
(ns infinity) { 0 0 moveto (a) <95 30 0001 7f800000> xshow } case
(cshow) { 20 30 moveto (abc) { cs } cshow } case
(cshow string first) { 20 30 moveto { cs } (ab) cshow } case
(cshow no current point) { (ab) { cs } cshow } case
(cshow scaled) { 2 3 scale 20 30 moveto (a) { cs } cshow } case
(cshow shows) { 20 30 moveto (abc) { pop pop 1 string dup 0 4 -1 roll put show } cshow } case
(cshow font back) { 0 0 moveto (ab) { cs currentfont /FontType get 3 eq out ( ) print /Helvetica 10 selectfont } cshow currentfont /FontType get 3 eq out ( ) print } case
(cshow stop) { { (ab) { pop pop pop /Helvetica 10 selectfont stop } cshow } stopped out ( ) print currentfont /FontType get 3 eq out ( ) print } case
(cshow empty) { 0 0 moveto () { (called) print } cshow } case
(cshow count) { 0 0 moveto 7 8 (ab) { pop pop pop count num } cshow } case
(cshow not a proc) { 0 0 moveto (ab) 5 cshow } case
(cshow literal proc) { 0 0 moveto (ab) { pop pop pop } cvlit cshow } case
(cshow array) { 0 0 moveto (ab) [1 2] cshow } case
(cshow exit) { 0 0 moveto (ab) { pop pop pop exit } cshow } case
(cshow error) { 0 0 moveto (ab) { pop pop pop 1 0 idiv } cshow } case
"#;

/// What Ghostscript 10.07.1 prints for `CASES`, one line per case.
const EXPECTED: &str = r#"
xshow => ok cp 80.0 30.0 
yshow => ok cp 20.0 90.0 
xyshow => ok cp 80.0 36.0 
xyshow scaled => ok cp 19.0 32.0 
xyshow rotated => ok cp 10.0 10.0 
xshow reals => ok cp 3.75 0.0 
xshow negative => ok cp 19.5 50.0 
extra numbers => ok cp 3.0 0.0 
packed array => ok cp 3.0 0.0 
empty => ok cp 5.0 6.0 
empty numstring => ok cp 5.0 6.0 
ns 16 be => ok cp 6.0 0.0 
ns 16 be frac => ok cp 6.5 0.0 
ns 16 le => ok cp 0.0 6.0 
ns 32 be => ok cp 6.0 0.0 
ns 32 be frac => ok cp 6.5 0.0 
ns 32 le => ok cp 6.0 0.0 
ns real be => ok cp 6.5 0.0 
ns real le => ok cp 6.5 0.0 
ns native le => ok cp 6.5 0.0 
ns signed => ok cp 0.0 0.0 
ns xyshow => ok cp 4.0 6.0 
ns stray byte => ok cp 6.0 0.0 
too few xshow => rangecheck
too few yshow => rangecheck
too few xyshow => rangecheck
non-number => typecheck
non-number yshow => typecheck
non-number xyshow => typecheck
unused non-number => typecheck
numarray not array => typecheck
text not string => typecheck
name numarray => typecheck
no current point => nocurrentpoint
rangecheck before nocurrentpoint => rangecheck
nocurrentpoint before typecheck => nocurrentpoint
ns count short => rangecheck
ns count long => rangecheck
ns too few for text => rangecheck
ns bad representation => rangecheck
ns bad header => typecheck
ns short header => typecheck
ns empty => typecheck
ns infinity => undefinedresult
cshow => cs 0.0 24.0 97 cs 0.0 28.0 98 cs 0.0 20.0 99 ok cp 20.0 30.0 
cshow string first => cs 0.0 24.0 97 cs 0.0 28.0 98 ok cp 20.0 30.0 
cshow no current point => cs 0.0 24.0 97 cs 0.0 28.0 98 ok nocp
cshow scaled => cs 0.0 24.0 97 ok cp 20.0 30.0 
cshow shows => ok cp 92.0 30.0 
cshow font back => cs 0.0 24.0 97 true cs 0.0 28.0 98 true true ok cp 0.0 0.0 
cshow stop => true true ok nocp
cshow empty => ok cp 0.0 0.0 
cshow count => 2 2 ok cp 0.0 0.0 
cshow not a proc => typecheck
cshow literal proc => typecheck
cshow array => typecheck
cshow exit => invalidexit
cshow error => undefinedresult
"#;

/// Run `body` after the prelude and return what it printed.
fn run_cases(body: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&format!("{PRELUDE}{body}"))
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

/// The same in Ghostscript.
fn gs_cases(body: &str) -> String {
    let dir = common::scratch_dir("xshow_gs_cases");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, format!("{PRELUDE}{body}")).expect("write ps");
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

/// Compare a report with `EXPECTED` line by line, naming the case.
fn assert_matches_pin(who: &str, report: &str) {
    let lines =
        |s: &'static str| -> Vec<&'static str> { s.lines().filter(|l| !l.is_empty()).collect() };
    let got: Vec<&str> = report.lines().collect();
    let (want, cases) = (lines(EXPECTED), lines(CASES));
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
fn pscat_prints_the_pinned_report() {
    assert_matches_pin("pscat", &run_cases(CASES));
}

#[test]
fn ghostscript_prints_the_pinned_report() {
    if !common::gs_available() {
        return;
    }
    assert_matches_pin("gs", &gs_cases(CASES));
}

// --- rendering ---------------------------------------------------------------

/// Every operator and both number forms, upright and under a rotated,
/// unevenly scaled CTM (the displacements are user space, so the glyphs
/// follow the CTM), and a cshow proc that places its own glyphs.
const SCENE: &str = "
    /BoxFont findfont 24 scalefont setfont
    10 170 moveto (abcde) [36 30 36 30 36] xshow
    170 20 moveto (abcde) [28 22 28 22 28] yshow
    gsave 100 40 translate 25 rotate 1 0.6 scale
      0 0 moveto (abcd) [30 8 26 -6 30 10 0 0] xyshow
    grestore
    /BoxFont findfont 18 scalefont setfont
    10 120 moveto (aaaa) <95 21 0004 0032 0028 0032 0028> xshow
    10 90 moveto (bcab) { pop pop 1 string dup 0 4 -1 roll put show 6 0 rmoveto } cshow
";

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(200, 200).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn ink(pm: &tiny_skia::Pixmap) -> usize {
    pm.pixels().iter().filter(|p| p.green() < 200).count()
}

/// Pixels whose colours differ by more than `over` (summed over R, G, B,
/// so 765 is black against white).
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

/// Rasterise `input` (PostScript or PDF) with gs at 200 x 200 pt.
fn gs_png(name: &str, input: &[u8]) -> tiny_skia::Pixmap {
    let dir = common::scratch_dir("xshow_gs_png");
    let (src, png) = (dir.join(name), dir.join("out.png"));
    std::fs::write(&src, input).expect("write input");
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
        "gs failed on {name}: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let pm = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_dir_all(&dir);
    pm
}

#[test]
fn ghostscript_draws_the_same_positioned_glyphs() {
    if !common::gs_available() {
        return;
    }
    let it = render(&format!("{PRELUDE}{SCENE}"));
    let ours = &it.gfx().pixmap;
    let gs = gs_png("scene.ps", format!("{PRELUDE}{SCENE} showpage").as_bytes());
    assert!(ink(ours) > 1000, "the glyphs are missing ({})", ink(ours));
    // gs's 16-level antialiasing and ours differ by up to about 430 on the
    // triangles' sloped edges; a glyph a pixel out of place differs by
    // nearly black against white along its whole outline.
    let n = differing(&gs, ours, 600);
    assert!(n < 5, "{n} pixels differ from gs");
}

#[test]
fn ghostscript_rasterises_our_pdf_of_the_scene_like_our_canvas() {
    if !common::gs_available() {
        return;
    }
    let it = render(&format!("{PRELUDE}{SCENE}"));
    let pdf = it.gfx().pdf_document().expect("pdf on");
    let theirs = gs_png("scene.pdf", &pdf);
    let n = differing(&theirs, &it.gfx().pixmap, 600);
    assert!(n < 5, "{n} pixels differ between our PDF and our canvas");
}

/// xshow places each glyph where a `moveto` and a one-glyph `show` would.
#[test]
fn each_glyph_lands_on_its_displacement() {
    for (positioned, by_hand) in [
        (
            "20 100 moveto (abc) [40 -25 60] xshow",
            "20 100 moveto (a) show 60 100 moveto (b) show 35 100 moveto (c) show",
        ),
        (
            "100 20 moveto (abc) [40 -15 60] yshow",
            "100 20 moveto (a) show 100 60 moveto (b) show 100 45 moveto (c) show",
        ),
        (
            "30 rotate 20 20 moveto (abc) [40 5 -10 30 60 0] xyshow",
            "30 rotate 20 20 moveto (a) show 60 25 moveto (b) show 50 55 moveto (c) show",
        ),
    ] {
        let setup = format!("{PRELUDE} /BoxFont findfont 20 scalefont setfont ");
        let a = render(&format!("{setup}{positioned}"));
        let b = render(&format!("{setup}{by_hand}"));
        assert!(ink(&a.gfx().pixmap) > 100, "{positioned}: nothing drawn");
        let n = differing(&a.gfx().pixmap, &b.gfx().pixmap, 96);
        assert!(n < 4, "{positioned}: {n} pixels differ from {by_hand}");
    }
}

#[test]
fn cshow_and_a_failed_xshow_paint_nothing() {
    for src in [
        "20 20 moveto (abc) { pop pop pop } cshow",
        "(abc) { pop pop pop } cshow",
        "20 20 moveto { (abc) [1 2] xshow } stopped pop",
        "20 20 moveto { (abc) [1 2 /x] xshow } stopped pop",
        "20 20 moveto { (abc) <95 20 0003 0001 0002 0003 0004> xyshow } stopped pop",
    ] {
        let it = render(&format!("{PRELUDE} box {src}"));
        assert_eq!(ink(&it.gfx().pixmap), 0, "{src} painted");
    }
}

/// A Type 3 glyph is built once per glyph, at the displaced pen: the
/// BuildChar records the device x of its glyph origin.
#[test]
fn buildchar_runs_at_each_displaced_pen() {
    let src = "
        /Xs [] def
        /Rec << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0] /FontBBox [0 0 1 1]
          /Encoding StandardEncoding
          /BuildChar { pop pop 500 0 setcharwidth
            /Xs [ Xs aload pop matrix currentmatrix 4 get ] store } >> definefont pop
        /Rec findfont 10 scalefont setfont
        10 0 moveto (abc) [7 11 13] xshow
        Xs aload pop currentpoint pop";
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let nums: Vec<f64> = it
        .operand_stack()
        .iter()
        .map(|o| match o.value {
            Value::Integer(i) => i as f64,
            Value::Real(r) => r,
            _ => panic!("not a number: {}", o.repr()),
        })
        .collect();
    assert_eq!(nums.len(), 4, "{nums:?}");
    for (got, want) in nums.iter().zip([10.0, 17.0, 28.0, 41.0]) {
        assert!((got - want).abs() < 1e-3, "{nums:?}");
    }
}

/// A Unicode-mode font (here `/UnicodeBuildChar true`) counts glyphs, and
/// cshow reports codes, as UTF-8 scalars, cut the way `show` cuts them.
#[test]
fn unicode_mode_fonts_count_and_report_scalars() {
    let font = "
        /U << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0] /UnicodeBuildChar true
          /BuildChar { pop pop 500 0 setcharwidth } >> definefont pop
        /U findfont 10 scalefont setfont ";
    // U+00E9 is two bytes: two glyphs need two numbers, not three.
    let ok = run_cases(&format!(
        "{font} 0 0 moveto (\u{e9}b) [3 4] xshow currentpoint exch num num"
    ));
    assert_eq!(ok.trim_end(), "7.0 0.0");
    let few = run_cases(&format!(
        "{font} 0 0 moveto {{ (\u{e9}b) [3] xshow }} stopped {{ $error /errorname get out }} if"
    ));
    assert_eq!(few, "rangecheck");
    let codes = run_cases(&format!("{font} (\u{e9}\u{AC00}) {{ cs }} cshow"));
    assert_eq!(codes.trim_end(), "cs 0.0 5.0 233 cs 0.0 5.0 44032");
}

// --- SVG text ----------------------------------------------------------------

fn text_svg(src: &str) -> String {
    let mut it = Interp::with_page(300, 200).expect("page");
    it.set_svg_text(SvgText::Text);
    it.enable_svg();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let pages = it.svg_pages().expect("recording enabled");
    assert_eq!(pages.len(), 1, "{pages:?}");
    pages.into_iter().next().unwrap_or_default()
}

/// The `<text ...>...</text>` elements of a page.
fn texts(svg: &str) -> Vec<&str> {
    svg.match_indices("<text ")
        .map(|(i, _)| {
            let end = svg[i..].find("</text>").expect("closed text") + i + "</text>".len();
            &svg[i..end]
        })
        .collect()
}

fn attr<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let i = text.find(&key)? + key.len();
    Some(&text[i..i + text[i..].find('"')?])
}

#[test]
fn svg_text_keeps_xshow_glyphs_as_text_at_their_displacements() {
    let svg = text_svg("/Helvetica 20 selectfont 10 50 moveto (abc) [30 40.5 50] xshow");
    assert!(!svg.contains("<path "), "no outlines left: {svg}");
    let t = texts(&svg);
    assert_eq!(t.len(), 1, "one run on one baseline: {svg}");
    assert!(t[0].ends_with(">abc</text>"), "{}", t[0]);
    assert_eq!(attr(t[0], "x"), Some("10 40 80.5"), "{svg}");
    assert_eq!(attr(t[0], "y"), Some("150"), "{svg}");
}

#[test]
fn svg_text_gives_each_yshow_glyph_its_own_baseline() {
    let svg = text_svg("/Helvetica 20 selectfont 10 50 moveto (abc) [30 40 50] yshow");
    let t = texts(&svg);
    assert_eq!(t.len(), 3, "{svg}");
    for (text, (ch, y)) in t.iter().zip([('a', "150"), ('b', "120"), ('c', "80")]) {
        assert!(text.ends_with(&format!(">{ch}</text>")), "{text}");
        assert_eq!(attr(text, "x"), Some("10"), "{text}");
        assert_eq!(attr(text, "y"), Some(y), "{text}");
    }
}
