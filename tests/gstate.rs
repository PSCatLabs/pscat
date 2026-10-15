//! Level 2 `gstate` objects (#394): `gstate`, `currentgstate`,
//! `setgstate`, and `copy` between gstates. The point of the issue is the
//! clip: a gstate carries the whole clip chain, so `setgstate` brings back
//! a nested, even-odd clip exactly, where `clippath` cannot.

use pscat::{Interp, PsError};

mod common;

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(120, 120).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it
}

fn eval(src: &str) -> Vec<String> {
    let it = run(src);
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn eval_err(src: &str) -> PsError {
    let mut it = Interp::with_page(120, 120).expect("test page");
    it.run_str(src).expect_err("expected an error")
}

fn pixels(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

/// A ring (even-odd: two concentric circles) nested inside a rotated
/// square. Neither link is an axis-aligned rect, and the ring needs its
/// rule, so `clippath` can hand back neither exactly.
const CLIPS: &str = "newpath 60 60 50 0 360 arc 60 60 25 0 360 arc eoclip \
     newpath 60 60 translate 30 rotate -40 -40 80 80 rectclip -30 rotate -60 -60 translate";
const FULL: &str = "newpath 0 0 moveto 120 0 lineto 120 120 lineto 0 120 lineto closepath";

#[test]
fn setgstate_brings_back_a_nested_eoclip_exactly() {
    let direct = run(&format!("gsave {CLIPS} {FULL} fill grestore"));
    // Capture under the clips, leave them, clip somewhere else, then
    // replay the capture.
    let replay = run(&format!(
        "gsave {CLIPS} /g gstate def grestore \
         newpath 0 0 moveto 30 0 lineto 30 30 lineto closepath clip \
         g setgstate {FULL} fill"
    ));
    assert!(
        pixels(&direct) == pixels(&replay),
        "a setgstate replay must paint byte-identically to the clip itself"
    );
    // currentgstate into an existing object is the same capture.
    let into = run(&format!(
        "/g gstate def gsave {CLIPS} g currentgstate pop grestore g setgstate {FULL} fill"
    ));
    assert!(pixels(&direct) == pixels(&into));

    // Controls. Replaying the region through `clippath ... clip` is only
    // a pixel trace of the mask (a superset at every anti-aliased edge),
    // and a nonzero ring fills the hole the eoclip rule makes.
    let via_clippath = run(&format!(
        "gsave {CLIPS} clippath /p [ {{ [ 3 1 roll /moveto ] }} {{ [ 3 1 roll /lineto ] }} \
         {{ [ 7 1 roll /curveto ] }} {{ [ /closepath ] }} pathforall ] def grestore \
         newpath p {{ aload pop cvx exec }} forall clip {FULL} fill"
    ));
    let nonzero = run(&format!(
        "gsave {} {FULL} fill grestore",
        CLIPS.replace("eoclip", "clip")
    ));
    let differ = |a: &Interp, b: &Interp| {
        pixels(a)
            .chunks(4)
            .zip(pixels(b).chunks(4))
            .filter(|(x, y)| x != y)
            .count()
    };
    let n = differ(&direct, &via_clippath);
    assert!(n > 200, "the clippath trace should be inexact: {n} px");
    let n = differ(&direct, &nonzero);
    assert!(n > 500, "the nonzero ring should fill the hole: {n} px");
    // The hole is unpainted in the replay, the ring painted.
    let px = |it: &Interp, x, y| it.gfx().pixmap.pixel(x, y).expect("in bounds").red();
    assert_eq!(px(&replay, 60, 60), 255, "the ring's hole");
    assert_eq!(px(&replay, 60, 95), 0, "the ring");
}

#[test]
fn setgstate_restores_path_ctm_colour_and_leaves_the_gsave_stack() {
    // Pinned against gs 10.07: the current path and CTM come back.
    assert_eq!(
        eval(
            "10 20 moveto 30 40 lineto 2 2 scale /h gstate def initgraphics newpath \
             h setgstate pathbbox 4 array astore matrix currentmatrix"
        ),
        ["[5.0 10.0 15.0 20.0]", "[2.0 0.0 0.0 -2.0 0.0 120.0]"]
    );
    assert_eq!(
        eval(
            "1 0 0 setrgbcolor 7 setlinewidth /k gstate def initgraphics k setgstate currentrgbcolor currentlinewidth"
        ),
        ["1.0", "0.0", "0.0", "7.0"]
    );
    // setgstate inside gsave/grestore: the grestore still pops what the
    // gsave pushed, so the outer state is untouched.
    assert_eq!(
        eval(
            "3 setlinewidth 9 setlinewidth /k gstate def 3 setlinewidth gsave k setgstate grestore currentlinewidth"
        ),
        ["3.0"]
    );
    assert_eq!(
        eval(
            "2 setlinewidth /k gstate def gsave 5 setlinewidth gsave k setgstate \
             grestore currentlinewidth grestore currentlinewidth"
        ),
        ["5.0", "2.0"]
    );
}

#[test]
fn gstate_objects_are_composite_with_identity() {
    assert_eq!(
        eval(
            "/g gstate def g type g 20 string cvs g g eq gstate g eq \
             g currentgstate g eq g xcheck"
        ),
        [
            "gstatetype",
            "(--nostringval--)",
            "true",
            "false",
            "true",
            "false"
        ]
    );
    // `==` prints `-gstate-`, as in gs.
    assert_eq!(eval("gstate"), ["-gstate-"]);
    let it = run("/g gstate def g ==");
    assert!(it.operand_stack().is_empty());
    // Copying a gstate onto itself is a no-op, not a double borrow.
    assert_eq!(eval("/g gstate def g g copy g eq"), ["true"]);
    // Usable as a dict key, by identity.
    assert_eq!(
        eval("/g gstate def /d 2 dict def d g 5 put d gstate 6 put d g get d length"),
        ["5", "2"]
    );
    // copy overwrites the destination and returns it.
    assert_eq!(
        eval(
            "4 setlinewidth /a gstate def 1 setlinewidth /b gstate def \
             a b copy b eq b setgstate currentlinewidth"
        ),
        ["true", "4.0"]
    );
}

#[test]
fn restore_rolls_back_a_gstate_overwrite() {
    // Pinned against gs 10.07: a currentgstate after save is undone.
    assert_eq!(
        eval(
            "/g gstate def save /s exch def 3 setlinewidth g currentgstate pop \
             s restore g setgstate currentlinewidth"
        ),
        ["1.0"]
    );
    assert_eq!(
        eval(
            "/g gstate def 5 setlinewidth /h gstate def save /s exch def h g copy pop \
             s restore g setgstate currentlinewidth"
        ),
        ["1.0"]
    );
}

#[test]
fn non_gstate_operands_are_typechecks() {
    assert_eq!(eval_err("1 setgstate"), PsError::Typecheck);
    assert_eq!(eval_err("1 currentgstate"), PsError::Typecheck);
    assert_eq!(eval_err("1 gstate copy"), PsError::Typecheck);
    assert_eq!(eval_err("gstate [ 1 ] copy"), PsError::Typecheck);
    assert_eq!(eval_err("setgstate"), PsError::StackUnderflow);
}

/// #488: `gstate .sameclip bool`, a pscat extension. True when the clip in
/// force is the very chain the gstate holds (or both have none); identity,
/// not geometry.
#[test]
fn sameclip_tests_clip_identity() {
    const R: &str = "0 0 50 50 rectclip";
    let yes = |src: &str| assert_eq!(eval(src), ["true"], "{src}");
    let no = |src: &str| assert_eq!(eval(src), ["false"], "{src}");
    yes("gstate .sameclip");
    yes(&format!("{R} gstate .sameclip"));
    no(&format!("gstate {R} .sameclip"));
    no(&format!("{R} gstate initclip .sameclip"));
    // a clip with the identical path is a new chain
    no(&format!("{R} gstate {R} .sameclip"));
    no(&format!("{R} gstate initclip {R} .sameclip"));
    // what puts the clip back puts the same chain back
    yes(&format!("{R} /g gstate def gsave {R} grestore g .sameclip"));
    yes(&format!(
        "{R} /g gstate def 0 0 9 9 rectclip g setgstate g .sameclip"
    ));
    yes(&format!(
        "{R} /g gstate def save initclip restore g .sameclip"
    ));
    yes(&format!(
        "{CLIPS} /g gstate def gsave initclip g setgstate grestore g .sameclip"
    ));
    // other graphics state does not count
    yes(&format!("{R} gstate 3 setlinewidth 0.5 setgray .sameclip"));
    assert_eq!(eval_err("1 .sameclip"), PsError::Typecheck);
    assert_eq!(eval(&format!("{{ 1 .sameclip }} stopped")), ["1", "true"]);
    assert_eq!(eval_err(".sameclip"), PsError::StackUnderflow);
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success())
}

/// gs (correctness reference) paints the same replayed region. Its
/// replay is byte-identical to its own direct clip, and pscat's agrees
/// with gs block for block. Pixel for pixel they differ at every edge,
/// with or without gstate: gs clips without anti-aliasing.
#[test]
fn ghostscript_agrees_on_a_replayed_nested_eoclip() {
    if !gs_available() {
        eprintln!("skipping gs gstate check: gs not installed");
        return;
    }
    let replay = format!(
        "gsave {CLIPS} /g gstate def grestore \
         newpath 0 0 moveto 30 0 lineto 30 30 lineto closepath clip \
         g setgstate {FULL} fill"
    );
    let direct = format!("{CLIPS} {FULL} fill");
    let dir = common::scratch_dir("pscat-gstate-gs");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let gs_grey = |name: &str, src: &str| -> Vec<u8> {
        let file = dir.join(format!("{name}.ps"));
        let png = dir.join(format!("{name}.png"));
        std::fs::write(&file, format!("{src}\nshowpage\n")).expect("write");
        let status = std::process::Command::new("gs")
            .args([
                "-dNOPAUSE",
                "-dBATCH",
                "-dNOSAFER",
                "-q",
                "-sDEVICE=pnggray",
                "-g120x120",
                "-r72",
                "-dGraphicsAlphaBits=4",
            ])
            .arg(format!("-o{}", png.display()))
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .status()
            .expect("run gs");
        assert!(status.success(), "gs rejected {name}");
        let img =
            tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
        img.pixels().iter().map(|p| p.red()).collect()
    };
    let gs_replay = gs_grey("replay", &replay);
    let gs_direct = gs_grey("direct", &direct);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        gs_replay == gs_direct,
        "gs's own replay should match its clip"
    );

    let grey =
        |it: &Interp| -> Vec<u8> { it.gfx().pixmap.pixels().iter().map(|p| p.red()).collect() };
    // Mean grey per 10x10 block, then the mean absolute block difference.
    let blocks = |g: &[u8]| {
        let mut b = vec![0f64; 144];
        for (i, v) in g.iter().enumerate() {
            b[(i / 120 / 10) * 12 + (i % 120) / 10] += f64::from(*v) / 100.0;
        }
        b
    };
    let diff = |a: &[u8], b: &[u8]| {
        blocks(a)
            .iter()
            .zip(blocks(b))
            .map(|(x, y)| (x - y).abs())
            .sum::<f64>()
            / 144.0
    };
    let same = diff(&grey(&run(&replay)), &gs_replay);
    let nonzero = diff(&grey(&run(&direct.replace("eoclip", "clip"))), &gs_replay);
    eprintln!("pscat vs gs: mean block diff {same:.2}; nonzero control {nonzero:.2}");
    assert!(same < 10.0, "pscat's replay disagrees with gs: {same:.2}");
    assert!(
        nonzero > same * 4.0,
        "the control is too close to gs: {nonzero:.2}"
    );
}
