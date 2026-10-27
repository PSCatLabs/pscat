//! Level 3 `clipsave`/`cliprestore` (#494). Ghostscript is the reference:
//! every case below was run through gs 10.07.1 first and its output pinned
//! here; `ghostscript_prints_the_pinned_lines` re-checks them where gs is
//! installed. The clip stack belongs to the `gsave` level (and travels in
//! gstate objects); `save` opens a level too; a `cliprestore` with no entry
//! at its level gives back the clip the level started with.

use pscat::Interp;

mod common;

/// `R` prints the clip's bbox (exact for nested `rectclip`s, #120);
/// `X` clips to 10..310 and opens a `gsave` level, so a level's starting
/// clip differs from the page.
const PRELUDE: &str = "/R { gsave clippath pathbbox grestore 4 array astore == } def \
                       /X { 10 10 300 300 rectclip gsave } def ";

type Case = (&'static str, &'static str, &'static [&'static str]);

/// (what, program, the lines gs prints)
const CASES: &[Case] = &[
    (
        "a pair puts the clip back",
        "R clipsave 10 10 50 50 rectclip R cliprestore R",
        &[
            "[0.0 0.0 612.0 792.0]",
            "[10.0 10.0 60.0 60.0]",
            "[0.0 0.0 612.0 792.0]",
        ],
    ),
    (
        "nested pairs pop newest first",
        "clipsave 10 10 300 300 rectclip clipsave 20 20 100 100 rectclip cliprestore R cliprestore R",
        &["[10.0 10.0 310.0 310.0]", "[0.0 0.0 612.0 792.0]"],
    ),
    (
        "an unpaired cliprestore at the top level clears the clip",
        "10 10 50 50 rectclip cliprestore R",
        &["[0.0 0.0 612.0 792.0]"],
    ),
    (
        "an unpaired cliprestore in a gsave gives back the clip at gsave",
        "10 10 300 300 rectclip gsave 20 20 50 50 rectclip cliprestore R grestore R",
        &["[10.0 10.0 310.0 310.0]", "[10.0 10.0 310.0 310.0]"],
    ),
    (
        "a deeper gsave level does not see the outer entry",
        "10 10 300 300 rectclip clipsave 20 20 200 200 rectclip R gsave 30 30 100 100 rectclip R cliprestore R grestore R cliprestore R",
        &[
            "[20.0 20.0 220.0 220.0]",
            "[30.0 30.0 130.0 130.0]",
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
            "[10.0 10.0 310.0 310.0]",
        ],
    ),
    (
        "grestore brings back the outer level entries",
        "clipsave 10 10 300 300 rectclip clipsave 20 20 200 200 rectclip gsave 30 30 100 100 rectclip cliprestore R cliprestore R grestore R cliprestore R cliprestore R",
        &[
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
            "[10.0 10.0 310.0 310.0]",
            "[0.0 0.0 612.0 792.0]",
        ],
    ),
    (
        "grestore drops the inner level entries",
        "10 10 300 300 rectclip gsave clipsave 20 20 50 50 rectclip grestore R cliprestore R",
        &["[10.0 10.0 310.0 310.0]", "[0.0 0.0 612.0 792.0]"],
    ),
    (
        "a cliprestore with no entry gives back the clip at gsave",
        "clipsave 10 10 50 50 rectclip gsave cliprestore R grestore R cliprestore R",
        &[
            "[10.0 10.0 60.0 60.0]",
            "[10.0 10.0 60.0 60.0]",
            "[0.0 0.0 612.0 792.0]",
        ],
    ),
    (
        "restore drops entries made since save",
        "save clipsave 10 10 50 50 rectclip restore R 10 10 50 50 rectclip cliprestore R",
        &["[0.0 0.0 612.0 792.0]", "[0.0 0.0 612.0 792.0]"],
    ),
    (
        "save starts a new level",
        "clipsave 10 10 50 50 rectclip save cliprestore R restore R cliprestore R",
        &[
            "[10.0 10.0 60.0 60.0]",
            "[10.0 10.0 60.0 60.0]",
            "[0.0 0.0 612.0 792.0]",
        ],
    ),
    (
        "restore brings back the save-time entries",
        "X 20 20 200 200 rectclip clipsave save 30 30 50 50 rectclip clipsave restore R 40 40 10 10 rectclip cliprestore R grestore",
        &["[20.0 20.0 220.0 220.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "inside a save, after a gsave pair, the empty fallback is the clip at save",
        "X 20 20 200 200 rectclip clipsave save 30 30 50 50 rectclip clipsave gsave grestore 40 40 5 5 rectclip cliprestore R cliprestore R restore R grestore",
        &[
            "[30.0 30.0 80.0 80.0]",
            "[20.0 20.0 220.0 220.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "grestoreall at a save boundary empties the stack",
        "X 20 20 200 200 rectclip clipsave 25 25 100 100 rectclip save 30 30 50 50 rectclip grestoreall R cliprestore R restore grestore",
        &["[25.0 25.0 125.0 125.0]", "[25.0 25.0 125.0 125.0]"],
    ),
    (
        "grestoreall at a save boundary",
        "X 20 20 200 200 rectclip clipsave save 30 30 50 50 rectclip clipsave 35 35 5 5 rectclip grestoreall R cliprestore R restore grestore",
        &["[20.0 20.0 220.0 220.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "only the clip changes: CTM, colour, path stay",
        "X clipsave 2 2 scale 0.5 setgray newpath 5 5 moveto 50 5 lineto 50 50 lineto closepath clip 7 7 moveto cliprestore R matrix currentmatrix == currentgray == currentpoint 2 array astore == grestore",
        &[
            "[5.0 5.0 155.0 155.0]",
            "[2.0 0.0 0.0 -2.0 0.0 792.0]",
            "0.5",
            "[7.0 7.0]",
        ],
    ),
    (
        "a gstate carries the stack",
        "X 20 20 200 200 rectclip clipsave 30 30 50 50 rectclip gstate /G exch def cliprestore R 40 40 100 100 rectclip G setgstate R cliprestore R grestore",
        &[
            "[20.0 20.0 220.0 220.0]",
            "[30.0 30.0 80.0 80.0]",
            "[20.0 20.0 220.0 220.0]",
        ],
    ),
    (
        "setgstate of an empty-stack gstate empties the stack",
        "X gstate /E exch def 20 20 50 50 rectclip clipsave 30 30 50 50 rectclip E setgstate R 35 35 10 10 rectclip cliprestore R grestore",
        &["[10.0 10.0 310.0 310.0]", "[10.0 10.0 310.0 310.0]"],
    ),
    (
        "currentgstate captures the stack",
        "X gstate /F exch def 20 20 200 200 rectclip clipsave 30 30 50 50 rectclip F currentgstate pop cliprestore R 40 40 10 10 rectclip F setgstate cliprestore R grestore",
        &["[20.0 20.0 220.0 220.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "initgraphics keeps the stack",
        "X 20 20 200 200 rectclip clipsave initgraphics R 30 30 50 50 rectclip cliprestore R grestore",
        &["[0.0 0.0 612.0 792.0]", "[20.0 20.0 220.0 220.0]"],
    ),
    (
        "initclip keeps the stack",
        "X 20 20 200 200 rectclip clipsave initclip 30 30 50 50 rectclip cliprestore R grestore",
        &["[20.0 20.0 220.0 220.0]"],
    ),
    (
        "both are operators and leave the stacks alone",
        "/clipsave load type == /cliprestore load type == clipsave cliprestore cliprestore count ==",
        &["operatortype", "operatortype", "0"],
    ),
];

/// The program run on a fresh letter-size page (gs's default), printing
/// into a buffer.
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
fn clipsave_and_cliprestore_print_what_ghostscript_prints() {
    for (what, src, want) in CASES {
        assert_eq!(pscat_lines(src), *want, "{what}: {src}");
    }
}

/// The restored clip is the saved chain itself, not a copy: a nested
/// even-odd clip comes back byte-identical, and `.sameclip` (#488) says it
/// is the very chain a gstate captured under it.
#[test]
fn cliprestore_brings_back_a_nested_eoclip_exactly() {
    const CLIPS: &str = "newpath 60 60 50 0 360 arc 60 60 25 0 360 arc eoclip \
         newpath 60 60 translate 30 rotate -40 -40 80 80 rectclip -30 rotate -60 -60 translate";
    const FULL: &str = "newpath 0 0 moveto 120 0 lineto 120 120 lineto 0 120 lineto closepath";
    let render = |src: &str| {
        let mut it = Interp::with_page(120, 120).expect("page");
        it.run_str(src)
            .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
        it
    };
    let direct = render(&format!("{CLIPS} {FULL} fill"));
    let restored = render(&format!(
        "{CLIPS} /g gstate def clipsave newpath 0 0 moveto 30 0 lineto 30 30 lineto closepath clip \
         cliprestore g .sameclip {FULL} fill"
    ));
    assert_eq!(
        restored.operand_stack().last().map(|o| o.repr()).as_deref(),
        Some("true"),
        "cliprestore should put back the very clip chain"
    );
    assert!(
        direct.gfx().pixmap.data() == restored.gfx().pixmap.data(),
        "the restored nested eoclip differs from the clip itself"
    );
    // Control: without the cliprestore the triangle clip stays.
    let kept = render(&format!(
        "{CLIPS} clipsave newpath 0 0 moveto 30 0 lineto 30 30 lineto closepath clip {FULL} fill"
    ));
    assert!(direct.gfx().pixmap.data() != kept.gfx().pixmap.data());
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
        eprintln!("skipping gs clipsave check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-clipsave-gs");
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
