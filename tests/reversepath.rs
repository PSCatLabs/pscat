//! `reversepath` (#1266, PLRM 3rd ed.): the current path traversed the other
//! way, each subpath keeping its open or closed state. Ghostscript 10.07.1
//! is the reference: each case was run through gs and its `pathforall` trace
//! (every coordinate, curve control points included) and `currentpoint` are
//! pinned here; `ghostscript_prints_the_pinned_traces` re-checks them where
//! gs is installed (CI requires it), and a whole-page render of a reversed
//! ring is compared with gs's.

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// Plain PostScript, valid in pscat and in gs: `dump` prints the current
/// path's `pathforall` elements with every coordinate, then the current
/// point (`@`) or `nocp`.
const PRELUDE: &str = r"
/pa { 32 string cvs print ( ) print } def
/dump { { 2 array astore (m ) print { pa } forall } { 2 array astore (l ) print { pa } forall }
        { 6 array astore (c ) print { pa } forall } { (z ) print } pathforall
        { currentpoint 2 array astore (@ ) print { pa } forall } stopped { (nocp) print } if
        (\n) print } def
/newcase { newpath } def
";

/// (what, program, gs's trace).
const CASES: &[(&str, &str, &str)] = &[
    (
        "an open polyline",
        "newcase 10 10 moveto 50 10 lineto 50 50 lineto reversepath dump",
        "m 50.0 50.0 l 50.0 10.0 l 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "a closed polyline: the current point is the reversed subpath's start",
        "newcase 10 10 moveto 50 10 lineto 50 50 lineto closepath reversepath dump",
        "m 50.0 50.0 l 50.0 10.0 l 10.0 10.0 z @ 50.0 50.0 \n",
    ),
    (
        "a curve then a line, open: control points swap",
        "newcase 10 10 moveto 20 30 40 30 50 10 curveto 70 20 lineto reversepath dump",
        "m 70.0 20.0 l 50.0 10.0 c 40.0 30.0 20.0 30.0 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "a closed curve with unequal control points",
        "newcase 10 10 moveto 20 30 40 31 50 12 curveto closepath reversepath dump",
        "m 50.0 12.0 c 40.0 31.0 20.0 30.0 10.0 10.0 z @ 50.0 12.0 \n",
    ),
    (
        "a line then a curve, closed",
        "newcase 10 10 moveto 50 10 lineto 60 30 70 31 80 12 curveto closepath reversepath dump",
        "m 80.0 12.0 c 70.0 31.0 60.0 30.0 50.0 10.0 l 10.0 10.0 z @ 80.0 12.0 \n",
    ),
    (
        "two curves, open",
        "newcase 0 0 moveto 1 2 3 4 5 6 curveto 7 8 9 10 11 12 curveto reversepath dump",
        "m 11.0 12.0 c 9.0 10.0 7.0 8.0 5.0 6.0 c 3.0 4.0 1.0 2.0 0.0 0.0 @ 0.0 0.0 \n",
    ),
    (
        "three subpaths, mixed: the order of subpaths reverses too",
        "newcase 10 10 moveto 50 10 lineto 50 50 lineto closepath 100 100 moveto 120 100 lineto \
         120 120 lineto 200 200 moveto 210 210 lineto closepath reversepath dump",
        "m 210.0 210.0 l 200.0 200.0 z m 120.0 120.0 l 120.0 100.0 l 100.0 100.0 \
         m 50.0 50.0 l 50.0 10.0 l 10.0 10.0 z @ 50.0 50.0 \n",
    ),
    (
        "the empty path: no error, and still no current point",
        "newcase reversepath dump",
        "nocp\n",
    ),
    (
        "a lone moveto is kept when it is the whole path",
        "newcase 10 10 moveto reversepath dump",
        "m 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "two movetos make one",
        "newcase 10 10 moveto 30 30 moveto reversepath dump",
        "m 30.0 30.0 @ 30.0 30.0 \n",
    ),
    (
        "a closed lone moveto",
        "newcase 10 10 moveto closepath reversepath dump",
        "m 10.0 10.0 z @ 10.0 10.0 \n",
    ),
    (
        "a trailing moveto after a closed subpath is dropped",
        "newcase 10 10 moveto 50 10 lineto closepath 70 70 moveto reversepath dump",
        "m 50.0 10.0 l 10.0 10.0 z @ 50.0 10.0 \n",
    ),
    (
        "a trailing moveto after an open subpath is dropped",
        "newcase 10 10 moveto 50 10 lineto 70 70 moveto reversepath dump",
        "m 50.0 10.0 l 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "two trailing movetos after a closed subpath",
        "newcase 10 10 moveto 50 10 lineto closepath 70 70 moveto 80 80 moveto reversepath dump",
        "m 50.0 10.0 l 10.0 10.0 z @ 50.0 10.0 \n",
    ),
    (
        "a lineto after closepath starts a subpath at the old start",
        "newcase 10 10 moveto 50 10 lineto 50 50 lineto closepath 30 5 lineto reversepath dump",
        "m 30.0 5.0 l 10.0 10.0 m 50.0 50.0 l 50.0 10.0 l 10.0 10.0 z @ 50.0 50.0 \n",
    ),
    (
        "that subpath closed again",
        "newcase 10 10 moveto 50 10 lineto 50 50 lineto closepath 30 5 lineto closepath \
         reversepath dump",
        "m 30.0 5.0 l 10.0 10.0 z m 50.0 50.0 l 50.0 10.0 l 10.0 10.0 z @ 50.0 50.0 \n",
    ),
    (
        "a curveto after closepath",
        "newcase 10 10 moveto 50 10 lineto closepath 30 5 40 6 50 7 curveto reversepath dump",
        "m 50.0 7.0 c 40.0 6.0 30.0 5.0 10.0 10.0 m 50.0 10.0 l 10.0 10.0 z @ 50.0 10.0 \n",
    ),
    (
        "reversing twice gives the path back",
        "newcase 10 10 moveto 50 10 lineto 20 30 40 30 50 10 curveto 70 70 moveto 80 80 lineto \
         closepath reversepath reversepath dump",
        "m 10.0 10.0 l 50.0 10.0 c 20.0 30.0 40.0 30.0 50.0 10.0 m 70.0 70.0 l 80.0 80.0 z \
         @ 70.0 70.0 \n",
    ),
    (
        "under a scale (a power of two, exact in device space)",
        "newcase 2 4 scale 10 10 moveto 50 10 lineto 20 30 40 30 50 10 curveto reversepath dump",
        "m 50.0 10.0 c 40.0 30.0 20.0 30.0 50.0 10.0 l 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "a lineto back to the start point",
        "newcase 10 10 moveto 10 10 lineto reversepath dump",
        "m 10.0 10.0 l 10.0 10.0 @ 10.0 10.0 \n",
    ),
];

fn pscat_trace(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&format!("{PRELUDE}{src}"))
        .unwrap_or_else(|e| panic!("pscat failed on {src:?}: {e}"));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

#[test]
fn reversepath_matches_the_pinned_ghostscript_traces() {
    for (what, src, want) in CASES {
        assert_eq!(&pscat_trace(src), want, "{what}: {src}");
    }
}

/// The pinned traces are gs's own.
#[test]
fn ghostscript_prints_the_pinned_traces() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("pscat-reversepath-gs");
    for (i, (what, src, want)) in CASES.iter().enumerate() {
        let file = dir.join(format!("c{i}.ps"));
        std::fs::write(&file, format!("{PRELUDE}{src}\n")).expect("write");
        let out = std::process::Command::new("gs")
            .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output_deadline()
            .expect("run gs");
        assert!(out.status.success(), "gs rejected {what}");
        let got = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(&got, want, "gs, {what}: {src}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reversepath_is_defined_and_takes_no_operands() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str("/reversepath where { pop (defined) = } { (undefined) = } ifelse")
        .expect("run");
    assert_eq!(it.captured_stdout(), b"defined\n");
    // The operand stack is left alone, with or without a path.
    for path in ["", "1 2 moveto 3 4 lineto"] {
        let mut it = Interp::new();
        it.capture_stdout();
        it.run_str(&format!("newpath {path} 7 (s) reversepath count = pop pop"))
            .expect("run");
        assert_eq!(it.captured_stdout(), b"2\n", "path {path:?}");
    }
}

/// A pscat-only invariant, on a CTM whose device points are not exact: two
/// reversals give the same `pathforall` back.
#[test]
fn reversepath_twice_is_the_identity_under_any_ctm() {
    let program = "newcase 13 7 translate 30 rotate 0.7 1.3 scale \
         1.1 2.2 moveto 5 6 lineto 7 8 9 3.3 4.4 9.9 curveto closepath \
         20 20 moveto 25 21 lineto 30 30 5 5 -3 7 curveto";
    let plain = pscat_trace(&format!("{program} dump"));
    let twice = pscat_trace(&format!("{program} reversepath reversepath dump"));
    assert_eq!(plain, twice);
}

// --- fill ---------------------------------------------------------------

const SIZE: u32 = 100;

/// Inner circle (r 20), reversed, then the outer (r 40), both centred on
/// (50, 50): the idiom the issue names. The reversed hole has winding
/// 1 - 1 = 0 under `fill`; unreversed it is 2 and the ring is a disc.
fn ring(reverse: &str, fill: &str) -> String {
    format!(
        "newpath 50 50 20 0 360 arc closepath {reverse} \
         50 90 moveto 50 50 40 90 450 arc closepath 0 setgray {fill}"
    )
}

fn page(src: &str) -> Interp {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

/// The gray of the pixel at user (x, y), y up.
fn gray(it: &Interp, x: u32, y: u32) -> u8 {
    it.gfx()
        .pixmap
        .pixel(x, SIZE - 1 - y)
        .expect("on the page")
        .red()
}

#[test]
fn a_reversed_inner_circle_makes_a_hole_under_the_nonzero_rule() {
    let holed = page(&ring("reversepath", "fill"));
    assert_eq!(gray(&holed, 50, 50), 255, "the hole is unpainted");
    assert_eq!(gray(&holed, 50 + 30, 50), 0, "the band is painted");
    assert_eq!(gray(&holed, 50 + 45, 50), 255, "outside is unpainted");
    // Without the reversal both circles wind the same way: a disc.
    let disc = page(&ring("", "fill"));
    assert_eq!(gray(&disc, 50, 50), 0);
    // The even-odd rule does not care which way a subpath runs.
    let eo = page(&ring("reversepath", "eofill"));
    let eo_plain = page(&ring("", "eofill"));
    assert_eq!(eo.gfx().pixmap.data(), eo_plain.gfx().pixmap.data());
    assert_eq!(gray(&eo, 50, 50), 255);
}

/// A reversed ring renders as gs renders it, away from the two circles'
/// edges (gs paints its edges hard, pscat anti-aliases them).
#[test]
fn a_reversed_ring_renders_as_ghostscript_renders_it() {
    if !common::gs_available() {
        return;
    }
    let ours = page(&ring("reversepath", "fill"));
    let dir = common::scratch_dir("pscat-reversepath-ring");
    let (ps, png) = (dir.join("ring.ps"), dir.join("ring.png"));
    std::fs::write(&ps, format!("{}\nshowpage\n", ring("reversepath", "fill"))).expect("write");
    let out = std::process::Command::new("gs")
        .args(["-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m", "-r72"])
        .arg(format!("-g{SIZE}x{SIZE}"))
        .arg(format!("-sOutputFile={}", png.display()))
        .arg(&ps)
        .stdin(std::process::Stdio::null())
        .output_deadline()
        .expect("run gs");
    assert!(out.status.success(), "gs failed");
    let theirs =
        tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("read png")).expect("decode");
    let (mut compared, mut painted) = (0, 0);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (f64::from(x) + 0.5 - 50.0, f64::from(y) + 0.5 - 50.0);
            let r = dx.hypot(dy);
            if (r - 20.0).abs() < 2.0 || (r - 40.0).abs() < 2.0 {
                continue;
            }
            let (a, b) = (
                ours.gfx().pixmap.pixel(x, y).expect("pixel").red(),
                theirs.pixel(x, y).expect("pixel").red(),
            );
            assert_eq!(a, b, "pixel ({x}, {y}) at radius {r:.1}");
            compared += 1;
            painted += usize::from(a == 0);
        }
    }
    assert!(compared > 6000 && painted > 2000, "{compared} {painted}");
    let _ = std::fs::remove_dir_all(&dir);
}
