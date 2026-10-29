//! `setpagedevice /PageSize` and `currentpagedevice` (#909). The behaviour
//! was pinned against Ghostscript 10.07.1 first: what an unusable
//! `/PageSize` raises, that every call erases the page, and that `grestore`
//! and `restore` bring the earlier page device back.
//!
//! Precedence, highest first: `--page` (an `Interp::with_page` caller, an
//! MCP width/height) > `setpagedevice /PageSize` > `%%BoundingBox` > letter.

mod common;
use common::CommandDeadline as _;

use std::process::{Command, Output};

use pscat::gfx::PageSizeSource;
use pscat::{Interp, PsError};
use tiny_skia::Pixmap;

/// The issue's repro: a red square on a 640x400 page.
const REPRO: &str = "%!PS\n<< /PageSize [640 400] >> setpagedevice\n\
     1 0 0 setrgbcolor\n20 20 100 100 rectfill\nshowpage\n";

fn pscat(args: &[&str], stdin: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(args)
        .output_with_input(stdin.as_bytes())
        .expect("run pscat")
}

/// Render `source` to PNG with `args`: the pages, and pscat's stderr.
fn render(tag: &str, args: &[&str], source: &str) -> (Vec<Pixmap>, String) {
    let dir = common::scratch_dir(tag);
    let out = dir.join("page.png");
    let out_str = out.to_str().expect("utf-8 path");
    let mut all = args.to_vec();
    all.extend(["--png", out_str, "-"]);
    let run = pscat(&all, source);
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "pscat {all:?} failed: {stderr}");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("list the scratch dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    names.sort();
    let pages = names
        .iter()
        .map(|p| Pixmap::decode_png(&std::fs::read(p).expect("read")).expect("decode"))
        .collect();
    (pages, stderr)
}

fn size(pm: &Pixmap) -> (u32, u32) {
    (pm.width(), pm.height())
}

fn rgb(pm: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let p = pm.pixel(x, y).expect("in bounds");
    (p.red(), p.green(), p.blue())
}

/// Dark pixels on a page.
fn ink(pm: &Pixmap) -> usize {
    pm.pixels().iter().filter(|p| p.red() < 128).count()
}

/// An interpreter whose page nothing chose (the CLI's default).
fn default_page() -> Interp {
    let it = Interp::new();
    assert_eq!(it.gfx().page_size_source(), PageSizeSource::Default);
    it
}

fn eval(it: &mut Interp, src: &str) -> Vec<String> {
    it.run_str(src).unwrap_or_else(|e| panic!("{src}: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

#[test]
fn the_issue_repro_is_a_red_square_on_a_640x400_page() {
    let (pages, stderr) = render("pd-repro", &[], REPRO);
    assert_eq!(pages.len(), 1);
    let pm = &pages[0];
    assert_eq!(size(pm), (640, 400));
    assert_eq!(rgb(pm, 70, 330), (255, 0, 0), "the square, 20..120 up");
    assert_eq!(rgb(pm, 300, 200), (255, 255, 255), "white elsewhere");
    assert!(!stderr.contains("Error"), "{stderr}");
    // The same with the size given explicitly too: they agree, no note.
    let (pages, stderr) = render("pd-repro-page", &["--page", "640x400"], REPRO);
    assert_eq!(size(&pages[0]), (640, 400));
    assert!(!stderr.contains("warning"), "{stderr}");
}

#[test]
fn the_operators_exist_and_leave_no_stack() {
    let mut it = default_page();
    assert_eq!(
        eval(
            &mut it,
            "systemdict /setpagedevice known systemdict /currentpagedevice known"
        ),
        ["true", "true"]
    );
    let mut it = default_page();
    assert_eq!(
        eval(
            &mut it,
            "<< /PageSize [640 400] >> setpagedevice count currentpagedevice /PageSize get"
        ),
        ["0", "[640 400]"]
    );
}

#[test]
fn currentpagedevice_reports_the_page_as_asked_for() {
    let mut it = default_page();
    assert_eq!(
        eval(&mut it, "currentpagedevice /PageSize get"),
        ["[612 792]"]
    );
    let mut it = default_page();
    // gs: [595.28 841.89], and a 595x842 canvas.
    assert_eq!(
        eval(
            &mut it,
            "<< /PageSize [595.28 841.89] >> setpagedevice currentpagedevice /PageSize get"
        ),
        ["[595.28 841.89]"]
    );
    assert_eq!(size(&it.gfx().pixmap), (595, 842));
    assert_eq!(it.gfx().page_size_source(), PageSizeSource::PageDevice);
    // Only /PageSize: it does not pretend to hold the other parameters.
    let mut it = default_page();
    assert_eq!(eval(&mut it, "currentpagedevice length"), ["1"]);
}

/// gs 10.07.1 raises configurationerror for every unusable /PageSize, and
/// typecheck for an operand that is not a dictionary. pscat also refuses a
/// three-element array, which gs reads two numbers from.
#[test]
fn an_unusable_page_size_is_refused_and_changes_nothing() {
    for bad in [
        "[0 400]",
        "[-10 400]",
        "[(a) 20]",
        "(ab)",
        "[100]",
        "[10 20 30]",
        "[1e300 400]",
        "[9000 400]",
        "[1e9 1e9]",
    ] {
        let mut it = default_page();
        it.run_str("0 0 10 10 rectfill").expect("paint");
        let src = format!("<< /PageSize {bad} >> setpagedevice");
        assert_eq!(it.run_str(&src), Err(PsError::ConfigurationError), "{bad}");
        assert_eq!(size(&it.gfx().pixmap), (612, 792), "{bad}");
        assert_eq!(it.operand_stack().len(), 1, "{bad}: the dict goes back");
        assert!(ink(&it.gfx().pixmap) > 0, "{bad}: the page was not erased");
    }
    let mut it = default_page();
    assert_eq!(it.run_str("5 setpagedevice"), Err(PsError::Typecheck));
    let mut it = default_page();
    assert_eq!(it.run_str("setpagedevice"), Err(PsError::StackUnderflow));
    // A program can catch it, as in gs.
    let mut it = default_page();
    let stack = eval(
        &mut it,
        "{ << /PageSize [0 0] >> setpagedevice } stopped $error /errorname get",
    );
    assert_eq!(stack[stack.len() - 2..], ["true", "/configurationerror"]);
}

#[test]
fn an_oversized_page_is_refused_by_the_cli() {
    // Over 8000 pixels a side, at 72 dpi and through --dpi.
    let run = pscat(&["-e", "<< /PageSize [9000 400] >> setpagedevice"], "");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(stderr.contains("configurationerror"), "{stderr}");
    assert!(stderr.contains("8000 pixels a side"), "{stderr}");
    let run = pscat(
        &[
            "--dpi",
            "720",
            "-e",
            "<< /PageSize [1000 400] >> setpagedevice",
        ],
        "",
    );
    assert!(!run.status.success(), "10000 pixels wide at 720 dpi");
    // Over --max-pixels: the line the MCP server reads back.
    let run = pscat(
        &[
            "--max-pixels",
            "300000",
            "--page",
            "100x100",
            "-e",
            "<< /PageSize [600 600] >> setpagedevice",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
    assert_eq!(
        pscat::limits::parse_pixel_cap_message(&stderr),
        None,
        "an explicit page is never resized, so nothing is over the cap: {stderr}"
    );
    let run = pscat(
        &[
            "--max-pixels",
            "300000",
            "--bbox",
            "-e",
            "%%BoundingBox: 0 0 100 100\n<< /PageSize [600 600] >> setpagedevice",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "{stderr}");
    assert_eq!(
        pscat::limits::parse_pixel_cap_message(&stderr),
        Some((600, 600, 300_000)),
        "{stderr}"
    );
    // The first page is held to the cap as well.
    let run = pscat(&["--max-pixels", "1000", "-e", "1 pop"], "");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert_eq!(
        pscat::limits::parse_pixel_cap_message(&stderr),
        Some((612, 792, 1000)),
        "{stderr}"
    );
}

/// A precedence case: tag, flags, source, the page it gets.
type Case<'a> = (&'a str, &'a [&'a str], String, (u32, u32));

/// --page > setpagedevice > %%BoundingBox > letter.
#[test]
fn the_precedence_matrix() {
    let eps = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 100 50 500 350\n%%EndComments\n";
    let set = "<< /PageSize [640 400] >> setpagedevice\n";
    let draw = "0 0 1 setrgbcolor 0 0 10 10 rectfill showpage\n";
    let cases: [Case; 6] = [
        ("default", &[], draw.to_string(), (612, 792)),
        ("set", &[], format!("{set}{draw}"), (640, 400)),
        ("bbox", &[], format!("{eps}{draw}"), (400, 300)),
        ("bbox-set", &[], format!("{eps}{set}{draw}"), (640, 400)),
        (
            "page-set",
            &["--page", "300x200"],
            format!("{set}{draw}"),
            (300, 200),
        ),
        (
            "page-bbox-set",
            &["--page", "300x200"],
            format!("{eps}{set}{draw}"),
            (300, 200),
        ),
    ];
    for (tag, args, source, want) in cases {
        let (pages, stderr) = render(&format!("pd-prec-{tag}"), args, &source);
        assert_eq!(size(&pages[0]), want, "{tag}: {stderr}");
        // setpagedevice puts the origin back at (0, 0): the square is in
        // the page's lower-left corner whichever rule set the size.
        if source.contains("setpagedevice") {
            assert_eq!(rgb(&pages[0], 2, want.1 - 3), (0, 0, 255), "{tag}");
        }
        let explicit_refused = args.contains(&"--page");
        assert_eq!(
            stderr.contains("/PageSize [640 400] ignored"),
            explicit_refused,
            "{tag}: {stderr}"
        );
    }
}

/// An embedding host that chose the size (`Interp::with_page`, as the
/// subcommands and the golden tests do) keeps it.
#[test]
fn a_with_page_caller_is_explicit() {
    let mut it = Interp::with_page(300, 200).expect("page");
    assert_eq!(it.gfx().page_size_source(), PageSizeSource::Explicit);
    assert_eq!(
        eval(
            &mut it,
            "<< /PageSize [640 400] >> setpagedevice currentpagedevice /PageSize get"
        ),
        ["[300 200]"]
    );
    assert_eq!(size(&it.gfx().pixmap), (300, 200));
    let mut it = Interp::with_page_scaled(300, 200, 2.0).expect("page");
    assert_eq!(it.gfx().page_size_source(), PageSizeSource::Explicit);
    eval(&mut it, "<< /PageSize [640 400] >> setpagedevice");
    assert_eq!(size(&it.gfx().pixmap), (600, 400));
}

/// gs erases the page on every setpagedevice, even `<< >>`, and resets
/// the graphics state as initgraphics does.
#[test]
fn setpagedevice_erases_and_does_initgraphics() {
    for (src, want) in [
        (
            "0 0 100 100 rectfill << /PageSize [200 200] >> setpagedevice",
            (200, 200),
        ),
        ("0 0 100 100 rectfill << >> setpagedevice", (612, 792)),
    ] {
        let mut it = default_page();
        it.run_str(src).expect(src);
        assert_eq!(size(&it.gfx().pixmap), want);
        assert_eq!(ink(&it.gfx().pixmap), 0, "{src}: erased");
        assert!(
            !it.gfx().has_trailing_art(),
            "{src}: marks discarded, not a page"
        );
    }
    let mut it = default_page();
    // gs: [1.0 0.0 0.0 -1.0 0.0 200.0], 0.0, 1.0.
    assert_eq!(
        eval(
            &mut it,
            "100 100 translate 0.5 setgray 3 setlinewidth \
             << /PageSize [200 200] >> setpagedevice \
             matrix currentmatrix currentgray currentlinewidth"
        ),
        ["[1.0 0.0 0.0 -1.0 0.0 200.0]", "0.0", "1.0"]
    );
    // nulldevice is left for the page device.
    let mut it = default_page();
    it.run_str("nulldevice << /PageSize [200 200] >> setpagedevice 0 0 10 10 rectfill")
        .expect("run");
    assert_eq!(ink(&it.gfx().pixmap), 100);
}

/// gs: the page device is part of the graphics state, so `grestore`,
/// `restore` and `setgstate` bring the earlier one back, erased.
#[test]
fn grestore_and_restore_bring_the_earlier_device_back() {
    for (what, src) in [
        (
            "grestore",
            "gsave << /PageSize [200 200] >> setpagedevice 0 0 50 50 rectfill grestore",
        ),
        (
            "restore",
            "save << /PageSize [200 200] >> setpagedevice 0 0 50 50 rectfill restore",
        ),
        (
            "grestoreall",
            "gsave gsave << /PageSize [200 200] >> setpagedevice 0 0 50 50 rectfill grestoreall",
        ),
        (
            "setgstate",
            "gstate << /PageSize [200 200] >> setpagedevice 0 0 50 50 rectfill setgstate",
        ),
    ] {
        let mut it = default_page();
        assert_eq!(
            eval(
                &mut it,
                &format!(
                    "{src} 0 0 20 20 rectfill currentpagedevice /PageSize get \
                     matrix currentmatrix"
                )
            ),
            ["[612 792]", "[1.0 0.0 0.0 -1.0 0.0 792.0]"],
            "{what}"
        );
        assert_eq!(size(&it.gfx().pixmap), (612, 792), "{what}");
        assert_eq!(ink(&it.gfx().pixmap), 400, "{what}: only the late square");
    }
    // A grestore or grestoreall with nothing saved comes back to the job's
    // initial state, which holds the device the job started on (gs: 612x792
    // after `<< /PageSize [200 200] >> setpagedevice grestore`).
    for op in ["grestore", "grestoreall"] {
        let mut it = default_page();
        assert_eq!(
            eval(
                &mut it,
                &format!(
                    "<< /PageSize [200 200] >> setpagedevice 0 0 50 50 rectfill {op} \
                     currentpagedevice /PageSize get"
                )
            ),
            ["[612 792]"],
            "{op}"
        );
        assert_eq!(size(&it.gfx().pixmap), (612, 792), "{op}");
        assert_eq!(ink(&it.gfx().pixmap), 0, "{op}: erased");
    }
    // An unbalanced grestore on a page that never changed device leaves
    // the page alone (#505's behaviour, unchanged).
    let mut it = default_page();
    eval(&mut it, "0 0 50 50 rectfill grestore grestoreall");
    assert_eq!(ink(&it.gfx().pixmap), 2500, "not erased");
    // The DSC per-page idiom: the page is shown at its size before the
    // restore takes the device back.
    let (pages, _) = render(
        "pd-pgsave",
        &[],
        "/pgsave save def << /PageSize [200 100] >> setpagedevice \
         0 0 50 50 rectfill showpage pgsave restore",
    );
    assert_eq!(pages.len(), 1);
    assert_eq!(size(&pages[0]), (200, 100));
    assert_eq!(ink(&pages[0]), 2500);
}

/// gs reinstalls the device a Type 3 glyph's `grestore` (pscat: its
/// snapshot restore) comes back to as well; a BuildChar that resizes the
/// page must not leave the canvas and the state disagreeing.
#[test]
fn a_glyph_that_sets_the_page_device_gives_it_back() {
    let mut it = default_page();
    it.run_str(
        "/F << /FontType 3 /FontMatrix [0.01 0 0 0.01 0 0] /FontBBox [0 0 100 100] \
         /Encoding StandardEncoding /BuildChar { pop pop 100 0 0 0 100 100 setcachedevice \
         << /PageSize [200 200] >> setpagedevice } >> definefont pop \
         /F findfont 10 scalefont setfont 100 100 moveto (a) show",
    )
    .expect("run");
    assert_eq!(size(&it.gfx().pixmap), (612, 792));
    assert_eq!(it.gfx().page_size(), (612.0, 792.0));
}

#[test]
fn other_keys_are_ignored_with_a_warning_once() {
    let run = pscat(
        &[
            "-e",
            "<< /Duplex true /PageSize [200 100] /HWResolution [300 300] >> setpagedevice \
             << /Duplex false >> setpagedevice currentpagedevice /Duplex known =",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{stderr}");
    assert_eq!(stdout.trim(), "false");
    assert_eq!(
        stderr.matches("/Duplex is not supported").count(),
        1,
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("/HWResolution is not supported").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("pscat: warning: setpagedevice:"),
        "{stderr}"
    );
}

#[test]
fn pages_of_different_sizes_in_png_svg_and_pdf() {
    let src = "<< /PageSize [200 100] >> setpagedevice 0 0 50 50 rectfill showpage \
               << /PageSize [100 300] >> setpagedevice 0 0 10 10 rectfill showpage";
    let (pages, _) = render("pd-multi", &[], src);
    assert_eq!(
        pages.iter().map(size).collect::<Vec<_>>(),
        [(200, 100), (100, 300)]
    );
    assert_eq!(ink(&pages[0]), 2500);
    assert_eq!(ink(&pages[1]), 100);

    let dir = common::scratch_dir("pd-multi-pdf");
    let pdf = dir.join("doc.pdf");
    let run = pscat(&["--pdf", pdf.to_str().expect("utf-8"), "-"], src);
    assert!(run.status.success());
    let bytes = std::fs::read(&pdf).expect("read the PDF");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/MediaBox [0 0 200 100]"), "first page");
    assert!(text.contains("/MediaBox [0 0 100 300]"), "second page");

    let svg = dir.join("doc.svg");
    let run = pscat(&["--svg", svg.to_str().expect("utf-8"), "-"], src);
    assert!(run.status.success());
    let first = std::fs::read_to_string(dir.join("doc-001.svg")).expect("page 1");
    let second = std::fs::read_to_string(dir.join("doc-002.svg")).expect("page 2");
    assert!(first.contains("width=\"200\" height=\"100\""), "{first}");
    assert!(second.contains("width=\"100\" height=\"300\""), "{second}");
}

/// `--bands` follows the page the program sets: the same PNG bytes as a
/// plain render, and `/BandBBox` describes the new page.
#[test]
fn bands_follow_a_page_size_change() {
    let src = "<< /PageSize [300 500] >> setpagedevice \
        0 1 49 { /i exch def i 50 div 0.3 1 sethsbcolor 0 i 10 mul 300 10 rectfill } for \
        0 0 1 setrgbcolor 150 250 100 0 360 arc fill showpage \
        << /PageSize [200 120] >> setpagedevice 1 0 0 setrgbcolor 10 10 180 100 rectfill showpage";
    let (plain, _) = render("pd-bands-plain", &[], src);
    let (banded, _) = render("pd-bands", &["--bands", "3"], src);
    assert_eq!(plain.len(), 2);
    for (k, (a, b)) in plain.iter().zip(&banded).enumerate() {
        assert_eq!(size(a), size(b), "page {k}");
        assert!(
            a.data() == b.data(),
            "page {k} differs from the plain render"
        );
    }
    // Band 0 of 6 (two a thread) on a 500-row page keeps rows 0..83.
    let run = pscat(
        &[
            "--bands",
            "3",
            "--png",
            common::scratch_dir("pd-bands-bbox")
                .join("p.png")
                .to_str()
                .expect("utf-8"),
            "-e",
            "<< /PageSize [300 500] >> setpagedevice BandBBox ==",
        ],
        "",
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("[0 417 300 500]"), "{stdout}");
}

/// A sweep's contact sheet takes its cells from the frames, which the
/// program's `setpagedevice` sizes.
#[test]
fn a_sweep_contact_sheet_takes_the_frames_size() {
    let dir = common::scratch_dir("pd-sweep");
    let sheet = dir.join("sheet.png");
    let run = pscat(
        &[
            "--sweep-seed",
            "1:2",
            "--contact-sheet",
            sheet.to_str().expect("utf-8"),
            "--grid",
            "2x1",
            "-",
        ],
        "<< /PageSize [100 50] >> setpagedevice rand pop 0 0 10 10 rectfill showpage",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
    let pm = Pixmap::decode_png(&std::fs::read(&sheet).expect("sheet")).expect("decode");
    assert!(
        pm.width() < 300 && pm.height() < 100,
        "{}x{}",
        pm.width(),
        pm.height()
    );
}

/// A new canvas is refused under `--max-memory` before it is allocated.
#[test]
fn a_resize_past_the_memory_cap_is_refused_before_allocating() {
    let run = pscat(
        &[
            "--max-memory",
            "20",
            "-e",
            "<< /PageSize [4000 4000] >> setpagedevice",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(stderr.contains("limitexceeded"), "{stderr}");
    assert!(stderr.contains("memory"), "{stderr}");
}

/// Codex round 1 on PR #930: a huge request under `--max-pixels` is a
/// catchable configurationerror, not an overflow; `--max-pixels` is
/// refused with `--spool`, which makes its own canvases.
#[test]
fn a_huge_request_under_a_pixel_cap_reports_without_overflow() {
    let run = pscat(
        &[
            "--max-pixels",
            "1000000",
            "-e",
            "{ << /PageSize [1e10 1e10] >> setpagedevice } stopped = \
             << /PageSize [1e10 1e10] >> setpagedevice",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(stdout.trim(), "true", "{stderr}");
    assert!(stderr.contains("configurationerror"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    let dir = common::scratch_dir("pd-spool");
    let run = pscat(
        &[
            "--spool",
            dir.to_str().expect("utf-8"),
            "--max-pixels",
            "1000",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(
        stderr.contains("--max-pixels doesn't combine with --spool"),
        "{stderr}"
    );
}

/// A sweep whose program sets a small page fits a sheet that the letter
/// page at the same dpi would not.
#[test]
fn a_sweep_sheet_is_sized_by_the_frames_not_the_default_page() {
    let dir = common::scratch_dir("pd-sweep-dpi");
    let sheet = dir.join("sheet.png");
    let run = pscat(
        &[
            "--dpi",
            "300",
            "--sweep-seed",
            "1:8",
            "--grid",
            "8x1",
            "--contact-sheet",
            sheet.to_str().expect("utf-8"),
            "-",
        ],
        "<< /PageSize [100 50] >> setpagedevice rand pop 0 0 10 10 rectfill showpage",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
}

/// A Type 3 BuildChar that sets the page device gives it back at the
/// glyph's end, and a band's `/BandBBox` follows it back too.
#[test]
fn a_glyph_restored_device_refreshes_band_bbox() {
    let out = common::scratch_dir("pd-glyph-band").join("p.png");
    let run = pscat(
        &[
            "--bands",
            "2",
            "--png",
            out.to_str().expect("utf-8"),
            "-e",
            "/F << /FontType 3 /FontMatrix [0.01 0 0 0.01 0 0] /FontBBox [0 0 100 100] \
             /Encoding StandardEncoding /BuildChar { pop pop 100 0 0 0 100 100 setcachedevice \
             << /PageSize [200 200] >> setpagedevice } >> definefont pop \
             /F findfont 10 scalefont setfont 100 100 moveto (a) show BandBBox ==",
        ],
        "",
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
    // Band 0 of 4 (two a thread) on the letter page: rows 0..198.
    assert!(stdout.contains("[0 594 612 792]"), "{stdout}");
}

/// Codex round 2 on PR #930, pinned against gs 10.07.1: a same-size
/// setpagedevice keeps the `clipsave` entries (`[10.0 20.0 40.0 60.0]` in
/// gs), and `currentpagedevice` is empty under `nulldevice` (gs: length 0).
#[test]
fn clipsave_survives_a_same_size_device_and_nulldevice_has_no_page() {
    let mut it = default_page();
    assert_eq!(
        eval(
            &mut it,
            "10 20 30 40 rectclip clipsave initclip << >> setpagedevice cliprestore \
             clippath pathbbox 4 array astore nulldevice currentpagedevice length"
        ),
        ["[10.0 20.0 40.0 60.0]", "0"]
    );
}

/// `--max-pixels` is checked before a sweep's or a banded run's sizing
/// probe allocates the first page.
#[test]
fn the_pixel_cap_is_checked_before_sizing_probes() {
    let dir = common::scratch_dir("pd-probe-cap");
    let sheet = dir.join("sheet.png");
    let png = dir.join("b.png");
    for args in [
        vec![
            "--max-pixels",
            "1",
            "--page",
            "100x100",
            "--sweep-seed",
            "1:2",
            "--contact-sheet",
            sheet.to_str().expect("utf-8"),
            "-",
        ],
        vec![
            "--max-pixels",
            "1",
            "--page",
            "100x100",
            "--bands",
            "2",
            "--png",
            png.to_str().expect("utf-8"),
            "-",
        ],
    ] {
        let run = pscat(&args, "0 0 10 10 rectfill");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "{args:?}");
        assert_eq!(
            pscat::limits::parse_pixel_cap_message(&stderr),
            Some((100, 100, 1)),
            "{args:?}: {stderr}"
        );
    }
}

/// Codex round 3 on PR #930: a restore that brings a larger canvas back is
/// held to `--max-memory` before the canvas is made.
#[test]
fn a_restored_device_is_held_to_the_memory_cap() {
    let run = pscat(
        &[
            "--max-memory",
            "20",
            "-e",
            "<< /PageSize [2000 2000] >> setpagedevice /big gstate def \
             << /PageSize [10 10] >> setpagedevice /small gstate def \
             /s 15000000 string def big setgstate small setgstate (done) =",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!run.status.success(), "{stdout}");
    assert!(!stdout.contains("done"), "{stdout}");
    assert!(stderr.contains("limitexceeded"), "{stderr}");
    assert!(stderr.contains("memory"), "{stderr}");
}

/// Codex round 4 on PR #930: an unsupported key is named without printing
/// it (an array key's text can be exponentially larger than the array), and
/// a refused restore stops only its own program.
#[test]
fn unsupported_keys_are_named_boundedly_and_a_refusal_does_not_outlive_its_program() {
    let run = pscat(
        &[
            "-e",
            "/a [0] def 40 { /a [a a] def } repeat << a 1 >> setpagedevice",
        ],
        "",
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
    assert!(
        stderr.contains("a arraytype key is not supported"),
        "{stderr}"
    );
    assert!(stderr.len() < 1000, "{} bytes", stderr.len());

    // The REPL runs each line as its own program.
    let run = pscat(
        &["--max-memory", "20"],
        "<< /PageSize [2000 2000] >> setpagedevice /big gstate def \
         << /PageSize [10 10] >> setpagedevice /s 15000000 string def big setgstate\n\
         (recovered) =\n",
    );
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(out.contains("limitexceeded (memory)"), "{out}");
    assert!(out.contains("recovered"), "{out}");
}

/// Codex round 5 on PR #930: a restore refused for memory leaves a state
/// that fits the canvas it kept, so the next program's page-sized image
/// does not read a clip mask of another size.
#[test]
fn a_refused_restore_leaves_a_consistent_state() {
    let run = pscat(
        &["--max-memory", "20"],
        "<< /PageSize [500 500] >> setpagedevice 10 10 100 100 rectclip /g gstate def \
         << /PageSize [2000 2000] >> setpagedevice /s 6000000 string def g setgstate\n\
         /str 100 string def 2000 2000 scale 100 100 8 [100 0 0 100 0 0] { str } image \
         (ok) =\n",
    );
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(out.contains("limitexceeded (memory)"), "{out}");
    assert!(out.contains("ok"), "{out}");
    assert!(!out.contains("panicked"), "{out}");
}

/// Codex round 6 on PR #930: a `cliprestore` that falls back to a clip made
/// on another page device remakes its mask for this canvas, so it still
/// clips. gs 10.07.1 paints nothing in both cases (the second clip's
/// device-space rectangle is off the new, shorter page).
#[test]
fn a_clip_restored_across_a_resize_still_clips() {
    for src in [
        "0 0 0 0 rectclip gsave << /PageSize [200 200] >> setpagedevice cliprestore \
         0 0 200 200 rectfill",
        "0 0 0 0 rectclip gsave << /PageSize [200 200] >> setpagedevice cliprestore \
         /str 10 string def 200 200 scale 10 10 8 [10 0 0 10 0 0] { str } image",
        "50 50 100 100 rectclip gsave << /PageSize [200 300] >> setpagedevice cliprestore \
         0 0 300 300 rectfill",
    ] {
        let mut it = default_page();
        it.run_str(src).expect(src);
        assert_eq!(ink(&it.gfx().pixmap), 0, "{src}");
    }
}

/// Codex round 7 on PR #930: restored onto a larger page, the clip keeps
/// the old page's bounds. gs 10.07.1 paints 10000 pixels at (0,0)..(99,99)
/// for the first and 8100 at (10,0)..(99,89) for the second.
#[test]
fn a_clip_restored_onto_a_larger_page_keeps_the_old_page_bounds() {
    for (src, want, first, last) in [
        (
            "<< /PageSize [100 100] >> setpagedevice gsave \
             << /PageSize [200 200] >> setpagedevice cliprestore 0 0 200 200 rectfill",
            10000,
            (0, 0),
            (99, 99),
        ),
        (
            "<< /PageSize [100 100] >> setpagedevice 10 10 500 500 rectclip gsave \
             << /PageSize [200 200] >> setpagedevice cliprestore 0 0 200 200 rectfill",
            8100,
            (10, 0),
            (99, 89),
        ),
    ] {
        let mut it = default_page();
        it.run_str(src).expect(src);
        let pm = &it.gfx().pixmap;
        let dark: Vec<(u32, u32)> = (0..pm.width())
            .flat_map(|x| (0..pm.height()).map(move |y| (x, y)))
            .filter(|&(x, y)| pm.pixel(x, y).is_some_and(|p| p.red() < 128))
            .collect();
        assert_eq!(dark.len(), want, "{src}");
        assert_eq!(dark.first(), Some(&first), "{src}");
        assert_eq!(dark.last(), Some(&last), "{src}");
    }
}
