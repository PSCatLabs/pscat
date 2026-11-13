//! `clippath` when the clip has no area (#1197). Ghostscript is the
//! reference: every case below was run through gs 10.07.1 first and its
//! output pinned here; `ghostscript_prints_the_pinned_lines` re-checks them
//! where gs is installed.
//!
//! gs gives an empty clip as one `moveto` at the device origin (`0 792` in
//! default user space), so `pathbbox` works on it; a zero-area rectangle
//! that lies inside the clip it was made under stays a box; an intersection
//! that only touches, or leaves the parent, is empty. Not covered, because
//! gs answers from integer device pixels there: a zero-height line that
//! only partly overlaps its parent comes back as a 1-pixel strip.

use pscat::Interp;

mod common;

/// `T` prints the `clippath` bbox, then its elements, one per line
/// (`nocurrentpoint` if `pathbbox` fails); `R` is `x y w h` as a path.
const PRELUDE: &str = "/R { 4 2 roll moveto 1 index 0 rlineto 0 exch rlineto neg 0 rlineto closepath } def \
    /p { 100 mul round 100 div 20 string cvs print ( ) print } def \
    /T { clippath mark { pathbbox } stopped { cleartomark (nocurrentpoint) = } \
    { 4 array astore { p } forall () = pop } ifelse \
    { (m ) print exch p p () = } { (l ) print exch p p () = } \
    { (c ) print 6 { p } repeat () = } { (z) = } pathforall } def\n";

type Case = (&'static str, &'static str, &'static [&'static str]);

/// (what, program, the lines gs prints)
const CASES: &[Case] = &[
    (
        "a zero-size rectclip at the origin",
        "newpath 0 0 0 0 rectclip T",
        &[
            "0.0 0.0 0.0 0.0",
            "m 0.0 0.0",
            "l 0.0 0.0",
            "l 0.0 0.0",
            "l 0.0 0.0",
            "z",
        ],
    ),
    (
        "a zero-size rectclip away from the origin",
        "newpath 10 20 0 0 rectclip T",
        &[
            "10.0 20.0 10.0 20.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
        ],
    ),
    (
        "a zero height with a negative width starts at the left end",
        "newpath 10 20 -5 0 rectclip T",
        &[
            "5.0 20.0 10.0 20.0",
            "m 5.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 5.0 20.0",
            "z",
        ],
    ),
    (
        "a zero width with a height",
        "newpath 10 20 0 30 rectclip T",
        &[
            "10.0 20.0 10.0 50.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 50.0",
            "l 10.0 50.0",
            "z",
        ],
    ),
    (
        "a zero width with a negative height keeps its order",
        "newpath 10 50 0 -30 rectclip T",
        &[
            "10.0 20.0 10.0 50.0",
            "m 10.0 50.0",
            "l 10.0 50.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
        ],
    ),
    (
        "a zero width reaching below the page is empty",
        "newpath 10 20 0 -30 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a zero width above the page is empty",
        "newpath 10 700 0 200 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "the array form with zero sizes",
        "newpath [10 20 0 0] rectclip T",
        &[
            "10.0 20.0 10.0 20.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
        ],
    ),
    (
        "a zero-size clip under a scale",
        "2 3 scale newpath 10 20 0 0 rectclip T",
        &[
            "10.0 20.0 10.0 20.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
        ],
    ),
    (
        "a zero-size clip under a mirror",
        "200 0 translate -1 1 scale newpath 10 20 -5 0 rectclip T",
        &[
            "5.0 20.0 10.0 20.0",
            "m 5.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 5.0 20.0",
            "z",
        ],
    ),
    (
        "two disjoint rectclips",
        "newpath 0 0 10 10 rectclip newpath 50 50 10 10 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a rectclip disjoint from a clip",
        "newpath 0 0 10 10 R clip newpath 50 50 10 10 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "two disjoint clips",
        "newpath 0 0 10 10 R clip newpath 50 50 10 10 R clip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "two disjoint eoclips",
        "newpath 0 0 10 10 R eoclip newpath 50 50 10 10 R eoclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "clip with an empty path",
        "newpath clip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "clip with a lone moveto",
        "newpath 10 20 moveto clip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "rectangles that only touch along an edge",
        "newpath 0 0 100 100 rectclip 100 0 50 50 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a third clip after touching rectangles",
        "newpath 0 0 100 100 rectclip 100 0 50 50 rectclip 0 0 500 500 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "an eoclip that cancels itself keeps its path",
        "newpath 10 10 20 20 R 10 10 20 20 R eoclip T",
        &[
            "10.0 10.0 30.0 30.0",
            "m 10.0 10.0",
            "l 30.0 10.0",
            "l 30.0 30.0",
            "l 10.0 30.0",
            "z",
            "m 10.0 10.0",
            "l 30.0 10.0",
            "l 30.0 30.0",
            "l 10.0 30.0",
            "z",
        ],
    ),
    (
        "a point inside a clip",
        "newpath 0 0 100 100 rectclip 10 20 0 0 rectclip T",
        &[
            "10.0 20.0 10.0 20.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
        ],
    ),
    (
        "a zero-width line inside a clip",
        "newpath 0 0 100 100 rectclip 10 20 0 30 rectclip T",
        &[
            "10.0 20.0 10.0 50.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 50.0",
            "l 10.0 50.0",
            "z",
        ],
    ),
    (
        "a point on the edge of a clip",
        "newpath 0 0 100 100 rectclip 100 50 0 0 rectclip T",
        &[
            "100.0 50.0 100.0 50.0",
            "m 100.0 50.0",
            "l 100.0 50.0",
            "l 100.0 50.0",
            "l 100.0 50.0",
            "z",
        ],
    ),
    (
        "a point on the corner of a clip",
        "newpath 0 0 100 100 rectclip 0 0 0 0 rectclip T",
        &[
            "0.0 0.0 0.0 0.0",
            "m 0.0 0.0",
            "l 0.0 0.0",
            "l 0.0 0.0",
            "l 0.0 0.0",
            "z",
        ],
    ),
    (
        "a line along the edge of a clip",
        "newpath 0 0 100 100 rectclip 100 10 0 20 rectclip T",
        &[
            "100.0 10.0 100.0 30.0",
            "m 100.0 10.0",
            "l 100.0 10.0",
            "l 100.0 30.0",
            "l 100.0 30.0",
            "z",
        ],
    ),
    (
        "a point outside a clip",
        "newpath 0 0 100 100 rectclip 200 200 0 0 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a line that leaves a clip",
        "newpath 0 0 100 100 rectclip 10 80 0 50 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a zero-size clip, then a bigger rectclip",
        "newpath 10 20 0 0 rectclip 0 0 100 100 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a zero-size clip, then the same again",
        "newpath 10 20 0 0 rectclip 10 20 0 0 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a zero-size clip between two bigger ones",
        "newpath 0 0 100 100 rectclip 10 20 0 0 rectclip 0 0 90 90 rectclip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a gsave keeps the empty clip, a grestore drops it",
        "0 0 100 100 rectclip gsave 10 20 0 0 rectclip gsave 50 50 5 5 rectclip T grestore T grestore T",
        &[
            "0.0 792.0 0.0 792.0",
            "m 0.0 792.0",
            "10.0 20.0 10.0 20.0",
            "m 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "l 10.0 20.0",
            "z",
            "0.0 0.0 100.0 100.0",
            "m 0.0 0.0",
            "l 100.0 0.0",
            "l 100.0 100.0",
            "l 0.0 100.0",
            "z",
        ],
    ),
    (
        "initclip after an empty clip",
        "newpath 0 0 10 10 rectclip 50 50 10 10 rectclip initclip T",
        &[
            "0.0 0.0 612.0 792.0",
            "m 0.0 792.0",
            "l 0.0 0.0",
            "l 612.0 0.0",
            "l 612.0 792.0",
            "z",
        ],
    ),
    (
        "a rectclip after an initclip after an empty clip",
        "newpath 0 0 10 10 rectclip 50 50 10 10 rectclip initclip 5 5 10 10 rectclip T",
        &[
            "5.0 5.0 15.0 15.0",
            "m 5.0 5.0",
            "l 15.0 5.0",
            "l 15.0 15.0",
            "l 5.0 15.0",
            "z",
        ],
    ),
    (
        "clippath twice on an empty clip",
        "newpath 0 0 10 10 rectclip 50 50 10 10 rectclip clippath T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "clippath clip on an empty clip stays empty",
        "newpath 0 0 10 10 rectclip 50 50 10 10 rectclip clippath clip T",
        &["0.0 792.0 0.0 792.0", "m 0.0 792.0"],
    ),
    (
        "a plain rectclip is unchanged",
        "newpath 10 20 5 7 rectclip T",
        &[
            "10.0 20.0 15.0 27.0",
            "m 10.0 20.0",
            "l 15.0 20.0",
            "l 15.0 27.0",
            "l 10.0 27.0",
            "z",
        ],
    ),
];

/// The program on a fresh letter-size page (gs's default), printing into a
/// buffer.
fn pscat_lines(src: &str) -> Vec<String> {
    let mut it = Interp::with_page(612, 792).expect("page");
    it.capture_stdout();
    it.run_str(&format!("{PRELUDE}{src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout())
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

#[test]
fn clippath_of_an_empty_clip_matches_ghostscript() {
    for (what, src, want) in CASES {
        assert_eq!(pscat_lines(src), *want, "{what}: {src}");
    }
}

/// `clippath` only changes the current path: the clip, and so what is
/// painted and what the SVG and PDF writers record, is the same with or
/// without it. Under an empty clip nothing is painted.
#[test]
fn an_empty_clip_still_paints_nothing_in_png_svg_and_pdf() {
    const CLIPS: &str = "0 0 10 10 rectclip 50 50 10 10 rectclip ";
    const PAINT: &str = "newpath 1 0 0 setrgbcolor 0 0 100 100 rectfill \
        20 20 moveto 80 80 lineto stroke showpage";
    let run = |src: &str| {
        let mut it = Interp::with_page(100, 100).expect("page");
        it.gfx_mut().enable_svg();
        it.gfx_mut().enable_pdf();
        it.run_str(src)
            .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
        it
    };
    let plain = run(&format!("{CLIPS}{PAINT}"));
    let probed = run(&format!("{CLIPS}clippath pathbbox pop pop pop pop {PAINT}"));
    assert_eq!(plain.gfx().svg_pages(), probed.gfx().svg_pages());
    assert_eq!(plain.gfx().pdf_document(), probed.gfx().pdf_document());
    // The page (rendered before showpage clears it) is blank: paint again
    // and look at the pixels.
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(&format!(
        "{CLIPS}clippath pathbbox pop pop pop pop newpath 1 0 0 setrgbcolor 0 0 100 100 rectfill"
    ))
    .expect("run");
    assert!(
        it.gfx()
            .pixmap
            .data()
            .chunks(4)
            .all(|p| p == [255, 255, 255, 255]),
        "an empty clip painted something"
    );
}

/// The pinned lines are gs's own (local only; CI has no gs).
#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("pscat-clippath-empty-gs");
    for (i, (what, src, want)) in CASES.iter().enumerate() {
        let file = dir.join(format!("c{i}.ps"));
        std::fs::write(&file, format!("{PRELUDE}{src}\n")).expect("write");
        let out = std::process::Command::new("gs")
            // The pins assume US Letter; Debian's gs defaults to A4 (libpaper), so say so.
            .args([
                "-q",
                "-dNOSAFER",
                "-dNODISPLAY",
                "-dBATCH",
                "-dNOPAUSE",
                "-sPAPERSIZE=letter",
            ])
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run gs");
        assert!(out.status.success(), "gs rejected {what}");
        let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect();
        assert_eq!(got, *want, "gs, {what}: {src}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
