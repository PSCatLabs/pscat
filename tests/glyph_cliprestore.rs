//! `cliprestore` and `grestore` inside a Type 3 `BuildChar` (#500).
//! Ghostscript runs the procedure inside a `gsave`, so a `cliprestore`
//! with no entry of its own gives back the clip at `show`, the glyph
//! starts with an empty `clipsave` stack, and an unmatched `grestore`
//! gives back the state at `show`. Every case below was run through gs
//! 10.07.1 first and its output pinned here;
//! `ghostscript_prints_the_pinned_lines` re-checks them where gs is
//! installed. What pscat still does differently is in
//! `notes/500-cliprestore-in-buildchar.md`.

use pscat::Interp;

mod common;

/// `R` prints the clip's bbox, `W` the line width. `X` clips to 10..310
/// and opens a `gsave`. `P` is the glyph procedure each case defines;
/// `go` shows one glyph of the font whose `BuildChar` runs it, with a
/// unit FontMatrix so glyph space is user space.
const PRELUDE: &str = "/R { gsave clippath pathbbox grestore 4 array astore == } def \
    /X { 10 10 300 300 rectclip gsave } def /W { currentlinewidth == } def \
    /P { } def /Fnt 8 dict def \
    Fnt begin /FontType 3 def /FontMatrix [1 0 0 1 0 0] def /FontBBox [0 0 1 1] def \
    /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for \
    Encoding 97 /a put /BuildChar { pop pop 1 0 setcharwidth P } def end \
    /F Fnt definefont pop /F findfont setfont /go { 0 0 moveto (a) show } def ";

type Case = (&'static str, &'static str, &'static [&'static str]);

/// (what, program, the lines gs prints)
const CASES: &[Case] = &[
    (
        "a cliprestore with no entry gives back the clip at show",
        "/P { cliprestore R } def X 20 20 200 200 rectclip go R grestore",
        &["[20.0 20.0 220.0 220.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "the glyph does not pop an entry made before show",
        "/P { cliprestore R } def X 20 20 200 200 rectclip clipsave 30 30 50 50 rectclip go R cliprestore R grestore",
        &[
            "[30.0 30.0 80.0 80.0]",
            "[30.0 30.0 80.0 80.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "a pair inside the glyph",
        "/P { 40 40 5 5 rectclip clipsave 45 45 1 1 rectclip cliprestore R cliprestore R } def X 20 20 200 200 rectclip go R grestore",
        &[
            "[40.0 40.0 45.0 45.0]",
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "a clipsave in the glyph does not outlive it",
        "/P { clipsave 1 1 1 1 rectclip } def X 20 20 200 200 rectclip clipsave 30 30 50 50 rectclip go R cliprestore R grestore",
        &["[30.0 30.0 80.0 80.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "a gsave in the glyph opens its own level",
        "/P { 40 40 50 50 rectclip clipsave 41 41 5 5 rectclip gsave 42 42 1 1 rectclip cliprestore R grestore R cliprestore R } def X 20 20 200 200 rectclip go R grestore",
        &[
            "[41.0 41.0 46.0 46.0]",
            "[41.0 41.0 46.0 46.0]",
            "[40.0 40.0 90.0 90.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "a save in the glyph",
        "/P { 40 40 5 5 rectclip save cliprestore R restore R cliprestore R } def X 20 20 200 200 rectclip go R grestore",
        &[
            "[40.0 40.0 45.0 45.0]",
            "[40.0 40.0 45.0 45.0]",
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "grestoreall in the glyph",
        "/P { 40 40 5 5 rectclip grestoreall R cliprestore R } def X 20 20 200 200 rectclip go",
        &["[0.0 0.0 612.0 792.0]", "[0.0 0.0 612.0 792.0]"],
    ),
    (
        "inside a gsave opened before show",
        "/P { 40 40 5 5 rectclip cliprestore R } def X 20 20 200 200 rectclip gsave 25 25 100 100 rectclip go R grestore",
        &["[25.0 25.0 125.0 125.0]", "[25.0 25.0 125.0 125.0]"],
    ),
    (
        "a nested show gives back its own clip at show",
        "/P { 70 70 20 20 rectclip clipsave 71 71 5 5 rectclip /P { cliprestore R } def go cliprestore R } def X 20 20 200 200 rectclip go R grestore",
        &[
            "[71.0 71.0 76.0 76.0]",
            "[70.0 70.0 90.0 90.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "an unmatched grestore gives back the state at show",
        "/P { 9 setlinewidth grestore W } def 3 setlinewidth go",
        &["3.0"],
    ),
    (
        "an unmatched grestore under a gsave",
        "/P { 9 setlinewidth grestore W } def 3 setlinewidth gsave 4 setlinewidth go",
        &["4.0"],
    ),
    (
        "an unmatched grestore restores the clip at show",
        "/P { 40 40 5 5 rectclip grestore R } def X 20 20 200 200 rectclip gsave 30 30 100 100 rectclip go",
        &["[30.0 30.0 130.0 130.0]"],
    ),
    (
        "a balanced gsave and grestore in the glyph",
        "/P { gsave 9 setlinewidth W grestore W } def 3 setlinewidth go W",
        &["9.0", "3.0", "3.0"],
    ),
    (
        "a leaked gsave is sealed at the end of the glyph",
        "/P { gsave 9 setlinewidth } def 3 setlinewidth gsave 4 setlinewidth go W grestore W",
        &["4.0", "3.0"],
    ),
];

fn pscat_lines(src: &str) -> Vec<String> {
    let mut it = Interp::with_page(612, 792).expect("page");
    it.capture_stdout();
    it.run_str(&format!("{PRELUDE}{src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout())
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_buildchar_runs_in_its_own_gsave_level_as_in_ghostscript() {
    for (what, src, want) in CASES {
        assert_eq!(pscat_lines(src), *want, "{what}: {src}");
    }
}

/// Painting: the glyph clips to a small box, fills, `cliprestore`s and
/// fills a page-sized box. The second fill lands under the clip at `show`
/// (the outer 20..120 square), so a pixel in it is painted and one outside
/// it is not. Before #500 the empty-stack `cliprestore` fell back to the
/// newest outer `gsave`, the page-wide clip, and the second fill spilled.
#[test]
fn a_glyph_cliprestore_paints_under_the_clip_at_show() {
    let src = "/Fnt 8 dict def Fnt begin /FontType 3 def /FontMatrix [1 0 0 1 0 0] def \
        /FontBBox [0 0 1 1] def /Encoding 256 array def \
        0 1 255 { Encoding exch /.notdef put } for Encoding 97 /a put \
        /BuildChar { pop pop 1 0 setcharwidth \
          40 40 10 10 rectclip 0 0 1 setrgbcolor 0 0 200 200 rectfill \
          cliprestore 1 0 0 setrgbcolor 0 0 200 200 rectfill } def end \
        /F Fnt definefont pop /F findfont setfont \
        gsave 20 20 100 100 rectclip 0 0 moveto (a) show grestore";
    let mut it = Interp::with_page(200, 200).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let pm = it.gfx().pixmap.clone();
    let px = |x: u32, y: u32| {
        let p = pm.pixel(x, y).expect("pixel");
        (p.red(), p.green(), p.blue(), p.alpha())
    };
    // Device y is flipped: user (60, 60) is device (60, 140).
    assert_eq!(px(60, 140), (255, 0, 0, 255), "inside the clip at show");
    assert_eq!(
        px(150, 50),
        (255, 255, 255, 255),
        "outside the clip at show stays blank"
    );
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The pinned lines are gs's own (local only; CI has no gs).
#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !gs_available() {
        eprintln!("skipping gs BuildChar check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-glyph-cliprestore-gs");
    for (i, (what, src, want)) in CASES.iter().enumerate() {
        let file = dir.join(format!("c{i}.ps"));
        std::fs::write(&file, format!("{PRELUDE}{src}\n")).expect("write");
        let out = std::process::Command::new("gs")
            .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run gs");
        assert!(out.status.success(), "gs rejected {what}");
        let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(got, *want, "gs, {what}: {src}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
