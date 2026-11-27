//! The `[utf8-in-byte-font]` check as host API (#1446): `set_utf8_text_check`
//! and `utf8_text_findings`. Each test uses only the supported surface
//! (`pscat::host`), as psanim's `check` would.

use pscat::host::{
    MAX_UTF8_TEXT_CHARS, MAX_UTF8_TEXT_FONTS, MAX_UTF8_TEXT_NAME_BYTES, Utf8TextFinding,
    Utf8TextReport,
};
use pscat::{Interp, Limits};

mod common;

const BYTE_FONT: &str = "/Helvetica findfont 12 scalefont setfont 5 40 moveto\n";
const UNICODE_FONT: &str = "/Helvetica findfont dup length dict copy dup /PSCatUnicode true put \
     /Helvetica-U exch definefont 12 scalefont setfont 5 40 moveto\n";

fn interp() -> Interp {
    Interp::with_page(80, 80).expect("valid page size")
}

fn one(report: &Utf8TextReport) -> &Utf8TextFinding {
    assert_eq!(report.fonts.len(), 1, "{report:?}");
    &report.fonts[0]
}

/// The issue's acceptance check: one finding with its line, and none
/// without the call.
#[test]
fn enabling_the_check_reads_one_finding_with_its_line() {
    let src = format!("{BYTE_FONT}(a\u{3c0}b) show");
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&src).expect("runs");
    let report = it.utf8_text_findings();
    let f = one(&report);
    assert_eq!(f.font, "Helvetica");
    assert_eq!(f.characters, ['\u{3c0}']);
    assert!(!f.more_characters);
    assert_eq!((f.line, f.file.as_deref()), (Some(2), None));
    assert_eq!(report.more_fonts, 0);

    let mut it = interp();
    it.run_str(&src).expect("runs");
    assert_eq!(it.utf8_text_findings(), Utf8TextReport::default());
}

#[test]
fn turning_the_check_off_forgets_and_stops() {
    let src = format!("{BYTE_FONT}(\u{3c0}) show");
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&src).expect("runs");
    assert_eq!(it.utf8_text_findings().fonts.len(), 1);
    it.set_utf8_text_check(false);
    assert!(it.utf8_text_findings().fonts.is_empty());
    it.run_str(&src).expect("runs");
    assert!(it.utf8_text_findings().fonts.is_empty());
    // And on again: the next program is the one that is reported.
    it.set_utf8_text_check(true);
    it.run_str(&src).expect("runs");
    assert_eq!(it.utf8_text_findings().fonts.len(), 1);
}

#[test]
fn a_font_that_reads_utf8_and_plain_text_are_silent() {
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&format!("{UNICODE_FONT}(a\u{3c0}b) show"))
        .expect("runs");
    assert!(it.utf8_text_findings().fonts.is_empty());
    it.run_str(&format!("{BYTE_FONT}(ascii) show"))
        .expect("runs");
    assert!(it.utf8_text_findings().fonts.is_empty());
}

/// The findings are the last program's: the next run forgets them, and a run
/// that failed keeps what it showed before the error.
#[test]
fn findings_belong_to_the_last_program_and_survive_its_error() {
    let mut it = interp();
    it.set_utf8_text_check(true);
    let err = it
        .run_str(&format!("{BYTE_FONT}(\u{3c0}) show 1 0 div"))
        .expect_err("division by zero");
    assert_eq!(err.name(), "undefinedresult");
    assert_eq!(one(&it.utf8_text_findings()).line, Some(2));
    it.run_str("1 pop").expect("runs");
    assert!(it.utf8_text_findings().fonts.is_empty());
}

/// `file` follows the conventions of `TraceFrame::file`: a `run` file's own
/// name, else the name the host gave the main program.
#[test]
fn the_file_is_a_run_file_or_the_hosts_main_name() {
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.set_main_file_name(Some("scene.ps"));
    it.run_str(&format!("{BYTE_FONT}(\u{3c0}) show"))
        .expect("runs");
    let report = it.utf8_text_findings();
    assert_eq!(
        (one(&report).file.as_deref(), one(&report).line),
        (Some("scene.ps"), Some(2))
    );

    it.add_virtual_file(
        "components/label.ps",
        format!("{BYTE_FONT}\n(\u{3c0}) show").into_bytes(),
    )
    .expect("registered");
    it.run_str("(components/label.ps) run").expect("runs");
    let report = it.utf8_text_findings();
    assert_eq!(
        (one(&report).file.as_deref(), one(&report).line),
        (Some("components/label.ps"), Some(3))
    );

    // Unnamed main program: no file.
    it.set_main_file_name(None);
    it.run_str(&format!("{BYTE_FONT}(\u{3c0}) show"))
        .expect("runs");
    assert_eq!(one(&it.utf8_text_findings()).file, None);
}

/// A show inside a procedure reports where execution was, the line that
/// called it, as `ErrorDetails::line` does.
#[test]
fn a_show_in_a_procedure_reports_the_line_that_ran_it() {
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&format!("{BYTE_FONT}/label {{\n(\u{3c0}) show\n}} def"))
        .expect("runs");
    assert!(it.utf8_text_findings().fonts.is_empty());
    it.run_str("\n\nlabel").expect("runs");
    assert_eq!(one(&it.utf8_text_findings()).line, Some(3));
}

/// The report is bounded: five fonts, six characters each, and a count of
/// the rest.
#[test]
fn the_report_is_bounded_in_fonts_and_characters() {
    let fonts = [
        "Helvetica",
        "Times-Roman",
        "Courier",
        "Palatino-Roman",
        "Bookman-Light",
        "AvantGarde-Book",
        "NewCenturySchlbk-Roman",
    ];
    let src: String = fonts
        .iter()
        .map(|f| format!("/{f} findfont 12 scalefont setfont 5 40 moveto (\u{3c0}) show\n"))
        .collect();
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&src).expect("runs");
    let report = it.utf8_text_findings();
    assert_eq!(report.fonts.len(), MAX_UTF8_TEXT_FONTS);
    assert_eq!(report.more_fonts, fonts.len() - MAX_UTF8_TEXT_FONTS);
    assert_eq!(report.fonts[0].font, "Helvetica");

    let text = "\u{3b1}\u{3b2}\u{3b3}\u{3b4}\u{3b5}\u{3b6}\u{3b7}\u{3b8}\u{3b1}";
    let mut it = interp();
    it.set_utf8_text_check(true);
    it.run_str(&format!("{BYTE_FONT}({text}) show"))
        .expect("runs");
    let report = it.utf8_text_findings();
    let f = one(&report);
    assert_eq!(f.characters.len(), MAX_UTF8_TEXT_CHARS);
    assert_eq!(f.characters[0], '\u{3b1}');
    assert!(f.more_characters);
}

/// A font dict's `FontName` can be any size; the name a finding carries is
/// cut, and only what is kept is copied.
#[test]
fn a_huge_font_name_is_cut() {
    let mut it = interp();
    it.set_utf8_text_check(true);
    for name in ["1048576 string", "70000 string cvn"] {
        it.run_str(&format!(
            "/Helvetica findfont dup length dict copy dup /FontName {name} put \
             /Huge exch definefont 12 scalefont setfont 5 40 moveto (\u{3c0}) show"
        ))
        .expect("runs");
        let report = it.utf8_text_findings();
        let f = one(&report);
        assert!(
            f.font.len() <= MAX_UTF8_TEXT_NAME_BYTES + 3,
            "{}",
            f.font.len()
        );
        assert!(f.font.ends_with("..."), "{:?}", f.font);
    }
}

/// Turning the check on changes nothing a program can see: the same pixels,
/// operand stack and step count, whether or not it found something.
#[test]
fn the_check_changes_no_output_and_no_step_count() {
    let src = format!(
        "{BYTE_FONT}20 {{ (a\u{3c0}b) show (plain) show 0 -6 rmoveto 5 40 moveto }} repeat \
         {UNICODE_FONT}(a\u{3c0}b) show 42 (kept)"
    );
    let run = |on: bool| {
        let mut it = interp();
        it.set_limits(Limits::none().with_max_steps(10_000_000))
            .expect("limits");
        it.set_utf8_text_check(on);
        it.run_str(&src).expect("runs");
        let kept = it.pop_string().expect("string");
        let n = it.pop_int().expect("int");
        (
            it.canvas_premultiplied_rgba8().to_vec(),
            kept,
            n,
            it.steps_used(),
            it.operand_count(),
        )
    };
    let off = run(false);
    let on = run(true);
    assert_eq!(off, on);
    assert!(off.3 > 0);
}
