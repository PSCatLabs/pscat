//! `currentcmykcolor`, `currentcolor`, `currentlinecap`, `currentlinejoin`,
//! `currentmiterlimit`, `currentstrokeadjust` and its setter (#513).
//! Ghostscript 10.07.1 is the reference: every program below was run
//! through gs first and its output pinned here;
//! `ghostscript_prints_the_pinned_lines` re-checks them where gs is
//! installed.
//!
//! Not pinned, because pscat differs from gs and the difference is not
//! about these operators: `currentrgbcolor` is unrounded on purpose (#515),
//! gs converts CMYK to RGB additively where pscat multiplies, and
//! `setcolorspace` leaves the painting colour alone where gs resets it
//! (see `notes/513-readback-operators.md`).

use pscat::Interp;

mod common;

/// `K` prints `currentcolor` as an array, `C` `currentcmykcolor`, `L` the
/// four line readbacks, and `T` runs a procedure and prints its error.
const PRELUDE: &str = "/K { currentcolor count array astore == } def \
     /C { currentcmykcolor 4 array astore == } def \
     /T { stopped { $error /errorname get == clear } if } def \
     /L { currentlinecap currentlinejoin currentmiterlimit currentstrokeadjust \
          4 array astore == } def ";

type Case = (&'static str, &'static str, &'static [&'static str]);

/// (what, program, the lines gs prints)
const CASES: &[Case] = &[
    (
        "a colour set in CMYK reads back as set",
        "0.1 0.2 0.3 0.4 setcmykcolor C K 1.5 -2 0.3 0.4 setcmykcolor C 1 0 0 0 setcmykcolor C K currentcolorspace ==",
        &[
            "[0.1 0.2 0.3 0.4]",
            "[0.1 0.2 0.3 0.4]",
            "[1.0 0.0 0.3 0.4]",
            "[1.0 0.0 0.0 0.0]",
            "[1.0 0.0 0.0 0.0]",
            "[/DeviceCMYK]",
        ],
    ),
    (
        "DeviceGray: black is the only component",
        "0.5 setgray C K 0.7 setgray C K 0 setgray C 1 setgray C",
        &[
            "[0.0 0.0 0.0 0.5]",
            "[0.5]",
            "[0.0 0.0 0.0 0.3]",
            "[0.7]",
            "[0.0 0.0 0.0 1.0]",
            "[0.0 0.0 0.0 0.0]",
        ],
    ),
    (
        "DeviceRGB: 1-r 1-g 1-b 0, no black generation",
        "0.3 0.6 0.9 setrgbcolor C 1 0 0 setrgbcolor C 0.2 0.4 0.6 setrgbcolor C 0 0 0 setrgbcolor C 1 1 1 setrgbcolor C 0 1 1 sethsbcolor C K",
        &[
            "[0.7 0.399999976 0.100000024 0.0]",
            "[0.0 1.0 1.0 0.0]",
            "[0.8 0.6 0.399999976 0.0]",
            "[1.0 1.0 1.0 0.0]",
            "[0.0 0.0 0.0 0.0]",
            "[0.0 1.0 1.0 0.0]",
            "[1.0 0.0 0.0]",
        ],
    ),
    (
        "currentcolor in DeviceRGB",
        "0.25 0.5 0.75 setrgbcolor K 0.3 0.6 0.9 setrgbcolor K currentcolorspace ==",
        &["[0.25 0.5 0.75]", "[0.3 0.6 0.9]", "[/DeviceRGB]"],
    ),
    (
        "setcolorspace gives the initial colour",
        "/DeviceCMYK setcolorspace C K 0.1 0.2 0.3 0.4 setcolor C K",
        &[
            "[0.0 0.0 0.0 1.0]",
            "[0.0 0.0 0.0 1.0]",
            "[0.1 0.2 0.3 0.4]",
            "[0.1 0.2 0.3 0.4]",
        ],
    ),
    (
        "Indexed over RGB: the index, as an integer",
        "[/Indexed /DeviceRGB 1 <ff0000 00ff00>] setcolorspace K C 1 setcolor K C",
        &["[0]", "[0.0 1.0 1.0 0.0]", "[1]", "[1.0 0.0 1.0 0.0]"],
    ),
    (
        "Indexed over Gray",
        "[/Indexed /DeviceGray 1 <40 c0>] setcolorspace 1 setcolor K C",
        &["[1]", "[0.0 0.0 0.0 0.247058809]"],
    ),
    (
        "Indexed over CMYK keeps the table's components",
        "[/Indexed /DeviceCMYK 1 <00000000 10203040>] setcolorspace C 1 setcolor K C",
        &[
            "[0.0 0.0 0.0 0.0]",
            "[1]",
            "[0.0627451 0.125490203 0.188235298 0.250980407]",
        ],
    ),
    (
        "Separation: the tint, and the alternate's CMYK",
        "[/Separation /Spot /DeviceCMYK {dup 0.5 mul exch 0 0}] setcolorspace K C 0.4 setcolor K C",
        &["[1.0]", "[0.5 1.0 0.0 0.0]", "[0.4]", "[0.2 0.4 0.0 0.0]"],
    ),
    (
        "Separation over Gray",
        "[/Separation /Spot /DeviceGray {1 exch sub}] setcolorspace 0.3 setcolor K C",
        &["[0.3]", "[0.0 0.0 0.0 0.3]"],
    ),
    (
        "Separation over RGB",
        "[/Separation /Spot /DeviceRGB {dup dup}] setcolorspace 0.3 setcolor K C",
        &["[0.3]", "[0.7 0.7 0.7 0.0]"],
    ),
    (
        "the colour is part of the graphics state",
        "0.1 0.2 0.3 0.4 setcmykcolor gsave 0.5 setgray grestore C K",
        &["[0.1 0.2 0.3 0.4]", "[0.1 0.2 0.3 0.4]"],
    ),
    ("line parameters at job start", "L", &["[0 0 10.0 true]"]),
    (
        "setlinecap, setlinejoin, setmiterlimit read back",
        "2 setlinecap 1 setlinejoin 3.3 setmiterlimit L 1 setmiterlimit L 7 setmiterlimit L 0 setlinecap 2 setlinejoin L",
        &[
            "[2 1 3.3 true]",
            "[2 1 1.0 true]",
            "[2 1 7.0 true]",
            "[0 2 7.0 true]",
        ],
    ),
    (
        "line parameters are part of the graphics state",
        "2 setlinecap 1 setlinejoin 3.3 setmiterlimit false setstrokeadjust gsave 0 setlinecap 0 setlinejoin 5 setmiterlimit true setstrokeadjust L grestore L",
        &["[0 0 5.0 true]", "[2 1 3.3 false]"],
    ),
    (
        "save and restore bring them back",
        "2 setlinecap false setstrokeadjust save 0 setlinecap true setstrokeadjust restore L",
        &["[2 0 10.0 false]"],
    ),
    (
        "initgraphics resets all but the stroke adjust flag",
        "2 setlinecap 1 setlinejoin 3.3 setmiterlimit false setstrokeadjust initgraphics L",
        &["[0 0 10.0 false]"],
    ),
    (
        "a grestore at the bottom comes back to the initial state",
        "2 setlinecap false setstrokeadjust grestore L",
        &["[0 0 10.0 true]"],
    ),
    (
        "a grestore at a save boundary keeps the save's",
        "2 setlinecap false setstrokeadjust save 0 setlinecap true setstrokeadjust grestore L restore",
        &["[2 0 10.0 false]"],
    ),
    (
        "bad operands leave the value alone",
        "2 setlinecap {3 setlinecap} T {-1 setlinejoin} T {3 setlinejoin} T {(a) setlinecap} T {1.4 setlinecap} T {0.5 setmiterlimit} T {1 setstrokeadjust} T {(a) setmiterlimit} T L",
        &[
            "/rangecheck",
            "/rangecheck",
            "/rangecheck",
            "/typecheck",
            "/typecheck",
            "/rangecheck",
            "/typecheck",
            "/typecheck",
            "[2 0 10.0 true]",
        ],
    ),
    (
        "the readbacks push exactly their results",
        "1 2 3 currentlinecap count ==  clear currentcolor count == clear currentcmykcolor count ==",
        &["4", "1", "4"],
    ),
];

/// gs prints a real with up to 9 significant digits, pscat its shortest
/// round-trip form. The `f32` values match, so compare the numbers inside
/// a line as `f32`s wherever gs's token is longer than 9 characters.
fn same_line(got: &str, want: &str) -> bool {
    if got == want {
        return true;
    }
    let tokens = |s: &str| -> Vec<String> {
        s.trim_matches(|c| c == '[' || c == ']')
            .split(' ')
            .map(str::to_string)
            .collect()
    };
    let (g, w) = (tokens(got), tokens(want));
    g.len() == w.len()
        && g.iter().zip(&w).all(|(g, w)| {
            g == w
                || matches!(
                    (g.parse::<f32>(), w.parse::<f32>()),
                    (Ok(a), Ok(b)) if a == b && w.len() > 9
                )
        })
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
fn the_readbacks_print_what_ghostscript_prints() {
    for (what, src, want) in CASES {
        let got = pscat_lines(src);
        assert!(
            got.len() == want.len() && got.iter().zip(*want).all(|(g, w)| same_line(g, w)),
            "{what}: {src}\n got {got:?}\nwant {want:?}"
        );
    }
}

/// A miter limit is held as `f32`; the readback is that `f32`'s shortest
/// decimal, and setting it again from the readback changes nothing.
#[test]
fn a_miter_limit_read_back_and_set_again_is_unchanged() {
    let got = pscat_lines(
        "3.3 setmiterlimit currentmiterlimit == currentmiterlimit setmiterlimit \
         currentmiterlimit == 1.1 setmiterlimit currentmiterlimit ==",
    );
    assert_eq!(got, ["3.3", "3.3", "1.1"]);
}

/// A colour read back with `currentcolor` and set again with `setcolor`
/// is the same colour (the rounding cannot move a colour that is only
/// copied), in each device space.
#[test]
fn a_colour_read_back_with_currentcolor_and_set_again_is_unchanged() {
    for set in [
        "0.123456789 setgray",
        "0.123456789 0.3 0.7 setrgbcolor",
        "0.123456789 0.3 0.7 0.9 setcmykcolor",
    ] {
        let got = pscat_lines(&format!(
            "{set} currentrgbcolor 3 array astore == currentcmykcolor 4 array astore == \
             currentcolor setcolor \
             currentrgbcolor 3 array astore == currentcmykcolor 4 array astore =="
        ));
        assert_eq!(got.len(), 4, "{set}: {got:?}");
        assert_eq!(got[..2], got[2..], "{set}");
    }
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
        eprintln!("skipping gs readback check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-readbacks-gs");
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
