//! `grestore`/`grestoreall` with no `gsave` to pop, and the `current*color`
//! readbacks (#505). Ghostscript is the reference: every case below was run
//! through gs 10.07.1 first and its output pinned here;
//! `ghostscript_prints_the_pinned_lines` re-checks them where gs is
//! installed.
//!
//! - At a `save` boundary (no `gsave` open since the innermost `save`),
//!   `grestore` brings back the state the `save` took and pops nothing, so
//!   a second `grestore` does the same again.
//! - At the bottom (nothing saved, no `save` live), `grestore` and
//!   `grestoreall` bring back the job's initial state.
//! - Either way the level starts with an empty `clipsave` stack.
//! - `currentgray`, `currenthsbcolor` and `currentrgbcolor` (#515) print the
//!   short value gs prints (`0.5`, `0.3`), not `0.49999999999999994` or
//!   `0.30000001192092896`.

use pscat::Interp;

mod common;

/// `R` prints the clip's bbox; `W` the line width.
const PRELUDE: &str = "/R { gsave clippath pathbbox grestore 4 array astore == } def \
                       /W { currentlinewidth == } def ";

type Case = (&'static str, &'static str, &'static [&'static str]);

/// (what, program, the lines gs prints)
const CASES: &[Case] = &[
    (
        "grestore at the bottom brings back the initial state",
        "5 setlinewidth 0.3 setgray 2 0 translate 10 10 moveto 10 10 50 50 rectclip grestore \
         W currentgray == matrix currentmatrix == { currentpoint } stopped == R",
        &[
            "1.0",
            "0.0",
            "[1.0 0.0 0.0 -1.0 0.0 792.0]",
            "true",
            "[0.0 0.0 612.0 792.0]",
        ],
    ),
    (
        "grestore at the bottom, after a gsave pair",
        "gsave 3 setlinewidth grestore 5 setlinewidth grestore W",
        &["1.0"],
    ),
    (
        "grestore past the last gsave reaches the initial state",
        "2 setlinewidth gsave 3 setlinewidth grestore grestore W",
        &["1.0"],
    ),
    (
        "grestore at a save boundary restores the save's state and stays there",
        "1 setlinewidth gsave 2 setlinewidth save 3 setlinewidth grestore W \
         4 setlinewidth grestore W restore W grestore W",
        &["2.0", "2.0", "2.0", "1.0"],
    ),
    (
        "a gsave inside the save pops first, then the boundary holds",
        "1 setlinewidth gsave 2 setlinewidth save 3 setlinewidth gsave 4 setlinewidth \
         grestore W grestore W grestore W restore W",
        &["3.0", "2.0", "2.0", "2.0"],
    ),
    (
        "a save at the top level",
        "5 setlinewidth save 6 setlinewidth grestore W restore W grestore W",
        &["5.0", "5.0", "1.0"],
    ),
    (
        "grestoreall with no save comes back to the initial state",
        "7 setlinewidth grestoreall W",
        &["1.0"],
    ),
    (
        "grestoreall with no save skips the oldest gsave's copy",
        "2 setlinewidth gsave 3 setlinewidth gsave 4 setlinewidth grestoreall W grestore W",
        &["1.0", "1.0"],
    ),
    (
        "grestoreall at a save boundary",
        "1 setlinewidth gsave 2 setlinewidth save 3 setlinewidth gsave 4 setlinewidth \
         grestoreall W grestoreall W restore W grestoreall W",
        &["2.0", "2.0", "2.0", "1.0"],
    ),
    (
        "grestore at the bottom empties the clipsave stack",
        "10 10 50 50 rectclip clipsave 20 20 5 5 rectclip grestore R cliprestore R",
        &["[0.0 0.0 612.0 792.0]", "[0.0 0.0 612.0 792.0]"],
    ),
    (
        "grestore at a save boundary brings back the clip at save",
        "10 10 50 50 rectclip save clipsave 20 20 5 5 rectclip grestore R cliprestore R",
        &["[10.0 10.0 60.0 60.0]", "[10.0 10.0 60.0 60.0]"],
    ),
    (
        "grestore at a save boundary does not bring back entries from before the save",
        "0 0 100 100 rectclip clipsave 10 10 50 50 rectclip save 20 20 5 5 rectclip \
         grestore R cliprestore R",
        &["[10.0 10.0 60.0 60.0]", "[10.0 10.0 60.0 60.0]"],
    ),
    (
        "currentgray of a gray",
        "0.5 setgray currentgray == 0.3 setgray currentgray == 0.25 setgray currentgray ==",
        &["0.5", "0.3", "0.25"],
    ),
    (
        "currentgray after a round trip through RGB and CMYK",
        "0.5 0.5 0.5 setrgbcolor currentgray == 0 0 0 0.5 setcmykcolor currentgray == \
         1 0 0 setrgbcolor currentgray == 0.2 0.4 0.6 setrgbcolor currentgray ==",
        &["0.5", "0.5", "0.3", "0.362000018"],
    ),
    (
        "currentrgbcolor of values exact in f32",
        "0.25 setgray currentrgbcolor 3 array astore == \
         0 0 0 0.5 setcmykcolor currentrgbcolor 3 array astore ==",
        &["[0.25 0.25 0.25]", "[0.5 0.5 0.5]"],
    ),
    (
        "currentrgbcolor rounds the f32s to their short decimals (#515)",
        "0.3 0.6 0.9 setrgbcolor currentrgbcolor 3 array astore == \
         0.123 0.456 0.789 setrgbcolor currentrgbcolor 3 array astore ==",
        &["[0.3 0.6 0.9]", "[0.123 0.456 0.789]"],
    ),
    (
        "currenthsbcolor",
        "1 0 0 setrgbcolor currenthsbcolor 3 array astore == 0 1 1 sethsbcolor \
         currentrgbcolor 3 array astore ==",
        &["[0.0 1.0 1.0]", "[1.0 0.0 0.0]"],
    ),
];

/// gs prints a real with up to 9 significant digits, pscat its shortest
/// round-trip form; the `f32` values match, so compare those lines as
/// numbers where gs's has more digits (only `0.362000018` above).
fn same_line(got: &str, want: &str) -> bool {
    got == want
        || matches!(
            (got.parse::<f32>(), want.parse::<f32>()),
            (Ok(a), Ok(b)) if a == b && want.len() > 9
        )
}

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
fn grestore_and_colour_readbacks_print_what_ghostscript_prints() {
    for (what, src, want) in CASES {
        let got = pscat_lines(src);
        assert!(
            got.len() == want.len() && got.iter().zip(*want).all(|(g, w)| same_line(g, w)),
            "{what}: {src}\n got {got:?}\nwant {want:?}"
        );
    }
}

/// The state `grestore` brings back at the bottom is the one the job
/// started in, readback for readback, on a scaled page too (`--dpi`).
#[test]
fn grestore_at_the_bottom_is_the_initial_state_at_any_scale() {
    const DUMP: &str = "currentlinewidth == currentgray == matrix currentmatrix == \
         currentdash 2 array astore == currentflat == currentalpha == currentblendmode == \
         { currentfont } stopped == currentcolorspace == R { currentpoint } stopped == ";
    const MESS: &str = "5 setlinewidth 0.3 0.6 0.9 setrgbcolor 2 3 translate 30 rotate \
         [4 2] 1 setdash 7 setflat 0.5 setalpha /Multiply setblendmode \
         /Helvetica 12 selectfont 10 10 50 50 rectclip 20 20 moveto ";
    for scale in [1.0, 2.0] {
        let mut it = Interp::with_page_scaled(612, 792, scale).expect("page");
        it.capture_stdout();
        it.run_str(&format!("{PRELUDE}{DUMP}{MESS}grestore {DUMP}"))
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let out = String::from_utf8_lossy(it.captured_stdout()).to_string();
        let lines: Vec<&str> = out.lines().collect();
        let (before, after) = lines.split_at(lines.len() / 2);
        assert_eq!(before, after, "scale {scale}");
    }
}

/// Reading a colour back and setting it again changes nothing, so the
/// rounding in the readbacks cannot move a colour that is only copied.
#[test]
fn a_colour_read_back_and_set_again_is_unchanged() {
    let mut it = Interp::with_page(10, 10).expect("page");
    it.capture_stdout();
    it.run_str(
        "0.123456789 0.3 0.7 setrgbcolor currentrgbcolor /b exch def /g exch def /r exch def \
         r g b setrgbcolor currentrgbcolor b eq exch g eq and exch r eq and == \
         0.123456789 setgray currentgray dup setgray currentgray eq ==",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        String::from_utf8_lossy(it.captured_stdout()),
        "true\ntrue\n"
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
        eprintln!("skipping gs grestore check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-grestore-gs");
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
