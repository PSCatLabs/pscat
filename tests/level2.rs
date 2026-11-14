//! packedarray, packing, clocks, languagelevel, resources — Stage 8
//! task 5. The fictions (plain arrays, resource sizes of -1) are
//! deliberate and documented in ops/level2.rs.

use pscat::{Interp, PsError};

mod common;
use common::CommandDeadline as _;

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn top_repr(it: &mut Interp) -> String {
    it.pop().expect("operand").repr()
}

#[test]
fn packedarray_collects_operands_in_order() {
    let mut it = run("1 2 3 3 packedarray");
    assert_eq!(top_repr(&mut it), "[1 2 3]");
}

#[test]
fn packing_flag_round_trips() {
    let mut it = run("currentpacking true setpacking currentpacking false setpacking");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "false", "default is false, per gs");
}

#[test]
fn clocks_advance_and_have_sane_types() {
    let mut it = run("usertime realtime");
    let rt: i64 = top_repr(&mut it).parse().expect("realtime int");
    let ut: i64 = top_repr(&mut it).parse().expect("usertime int");
    assert!((0..60_000).contains(&ut), "fresh interp usertime, got {ut}");
    assert!(rt > 1_500_000_000_000, "epoch millis, got {rt}");
}

#[test]
fn languagelevel_is_two() {
    let mut it = run("languagelevel");
    assert_eq!(top_repr(&mut it), "2");
}

#[test]
fn identity_operators_name_pscat_and_its_version() {
    // Issue #1048: PLRM `product`, `version`, `revision`, `serialnumber`.
    let mut it = run("product version revision serialnumber");
    assert_eq!(top_repr(&mut it), "0");
    let major: i64 = env!("CARGO_PKG_VERSION_MAJOR").parse().expect("major");
    let minor: i64 = env!("CARGO_PKG_VERSION_MINOR").parse().expect("minor");
    let patch: i64 = env!("CARGO_PKG_VERSION_PATCH").parse().expect("patch");
    let revision = major * 10000 + minor * 100 + patch;
    assert_eq!(top_repr(&mut it), revision.to_string());
    assert_eq!(
        top_repr(&mut it),
        format!("({})", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(top_repr(&mut it), "(pscat)");
}

#[test]
fn revision_gates_a_feature_on_a_minimum_version() {
    // The use the issue asks for: "is this pscat 0.7.0 or later?",
    // safe on an interpreter without the operator too.
    let mut it = run("/revision where { pop revision 700 ge } { false } ifelse");
    assert_eq!(top_repr(&mut it), "true");
}

#[test]
fn generic_resource_round_trip() {
    let mut it = run("/pal [1 2 3] /MyCategory defineresource pop
         /pal /MyCategory findresource
         /pal /MyCategory resourcestatus");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "-1", "size unknown, as gs reports it");
    assert_eq!(top_repr(&mut it), "0", "status 0: in VM");
    assert_eq!(top_repr(&mut it), "[1 2 3]");
}

#[test]
fn missing_resource_is_undefinedresource() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("/nope /MyCategory findresource"),
        Err(PsError::UndefinedResource)
    );
}

#[test]
fn undefineresource_removes() {
    let mut it = run("/x 42 /Cat defineresource pop
         /x /Cat undefineresource
         /x /Cat resourcestatus");
    assert_eq!(top_repr(&mut it), "false");
}

#[test]
fn font_category_delegates_to_findfont() {
    // findresource /Font behaves as findfont (substitution included);
    // the result is usable with scalefont/setfont.
    let mut it = run("/Helvetica /Font findresource 12 scalefont setfont
         /Helvetica /Font resourcestatus");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "-1");
    assert_eq!(top_repr(&mut it), "0");
}

#[test]
fn font_defineresource_installs_in_fontdirectory() {
    let mut it = run("/F 5 dict dup begin
           /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def
           /BuildChar { pop pop } def
         end /Font defineresource pop
         FontDirectory /F known");
    assert_eq!(top_repr(&mut it), "true");
}

#[test]
fn resources_roll_back_with_restore() {
    // defineresource writes are journaled like any dict write.
    let mut it = run("/a 1 /Cat defineresource pop
         save /b 2 /Cat defineresource pop restore
         /b /Cat resourcestatus
         /a /Cat resourcestatus");
    assert_eq!(top_repr(&mut it), "true", "pre-save resource survives");
    assert_eq!(top_repr(&mut it), "-1");
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "false", "post-save resource rolled back");
}

#[test]
fn resourcestatus_reports_every_font_findfont_can_load() {
    // Issue #1214. Ghostscript 10.07.1 (`/Name /Font resourcestatus`):
    // status 2, size -1 for a face not yet loaded, false for an unknown
    // name. The standard 35 are all compiled in, so they are available on
    // any install. Before #1214 only the twelve Liberation-backed faces
    // (Helvetica, Courier-BoldOblique) read true; the rest (NewCentury,
    // Palatino, Bookman, AvantGarde, ZapfChancery, Helvetica-Narrow,
    // Symbol, ZapfDingbats) read false.
    for face in [
        "/Helvetica",
        "/Courier-BoldOblique",
        "/NewCenturySchlbk-Roman",
        "/Palatino-Italic",
        "/Bookman-Demi",
        "/AvantGarde-Book",
        "/ZapfChancery-MediumItalic",
        "/Helvetica-Narrow-Bold",
        "/Symbol",
        "/ZapfDingbats",
        "(Helvetica)",
        "(NewCenturySchlbk-Roman)",
    ] {
        let mut it = run(&format!("{face} /Font resourcestatus"));
        assert_eq!(top_repr(&mut it), "true", "{face}");
        assert_eq!(top_repr(&mut it), "-1", "{face}: size");
        assert_eq!(top_repr(&mut it), "2", "{face}: status");
    }
}

#[test]
fn resourcestatus_is_false_for_a_name_nothing_provides() {
    // Font-pack faces of an uninstalled pack are pinned against a bare
    // install in tests/font_packs.rs.
    for face in ["/NoSuchFont-Foo", "(NoSuchFont-Foo)", "/Nope"] {
        let mut it = run(&format!("{face} /Font resourcestatus"));
        assert_eq!(top_repr(&mut it), "false", "{face}");
        assert_eq!(it.operand_stack().len(), 0, "{face}: only the boolean");
    }
}

#[test]
fn resourcestatus_agrees_with_findfont_and_findresource() {
    // Everything resourcestatus reports as available, findresource
    // returns as the face itself (not a stand-in): FontName is not
    // Helvetica unless the name is. Defined and loaded fonts are in VM.
    let mut it = run("/NewCenturySchlbk-Roman /Font resourcestatus pop pop pop
         /NewCenturySchlbk-Roman /Font findresource /FontName get
         /NewCenturySchlbk-Roman /Font resourcestatus pop pop");
    assert_eq!(top_repr(&mut it), "0", "findfont put it in FontDirectory");
    assert_eq!(top_repr(&mut it), "/texgyreschola-regular");

    // A name findfont does not know is substituted, and then in VM, as gs.
    let mut it = run("/Zz-Unknown /Font resourcestatus
         /Zz-Unknown findfont pop
         /Zz-Unknown /Font resourcestatus pop pop");
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "false");
}

#[test]
fn resourcestatus_sees_a_font_defined_with_definefont() {
    let mut it = run("/MyF 5 dict dup begin
           /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def /BuildChar { pop pop } def
         end definefont pop
         /MyF /Font resourcestatus
         (MyF) /Font resourcestatus
         /MyF /Font undefineresource
         /MyF /Font resourcestatus");
    assert_eq!(top_repr(&mut it), "false", "undefined again");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "-1");
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "-1");
    assert_eq!(top_repr(&mut it), "0");
}

#[test]
fn undefining_a_builtin_font_leaves_it_available() {
    // gs: after `/Helvetica /Font undefineresource`, status is 2 again,
    // because the face can still be loaded.
    let mut it = run("/Helvetica findfont pop
         /Helvetica /Font undefineresource
         /Helvetica /Font resourcestatus");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "-1");
    assert_eq!(top_repr(&mut it), "2");
}

/// Prints `status size` on one line, or `false`, for each case. These are
/// the cases where pscat and gs agree; a face `findfont` has already
/// loaded is not among them (gs: status 1 with the file size, pscat: 0 -1).
const STATUS_PROGRAM: &str = "/Q { resourcestatus { exch 20 string cvs print ( ) print 20 string cvs print } { (false) print } ifelse () = } def \
    /Helvetica /Font Q /NewCenturySchlbk-Roman /Font Q /Symbol /Font Q (Palatino-Roman) /Font Q \
    /NoSuchFont-Foo /Font Q (NoSuchFont-Foo) /Font Q \
    /MyF 5 dict dup begin /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def \
      /Encoding 256 array def /BuildChar { pop pop } def end definefont pop \
    /MyF /Font Q (MyF) /Font Q /MyF /Font undefineresource /MyF /Font Q \
    /K 5 /Generic defineresource pop /K /Generic Q /K2 /Generic Q /K /Generic undefineresource /K /Generic Q\n";

#[test]
fn resourcestatus_matches_ghostscript() {
    // Issue #1214: expected lines are what gs 10.07.1 prints for the same
    // program; the second half re-runs gs where it is installed.
    let want = "2 -1\n2 -1\n2 -1\n2 -1\nfalse\nfalse\n0 -1\n0 -1\nfalse\n0 -1\nfalse\nfalse\n";
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["-e", STATUS_PROGRAM])
        .output_deadline()
        .expect("run pscat");
    assert_eq!(String::from_utf8_lossy(&out.stdout), want);
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("level2-resourcestatus-gs");
    let file = dir.join("status.ps");
    std::fs::write(&file, STATUS_PROGRAM).expect("write");
    let gs = std::process::Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .output_deadline()
        .expect("run gs");
    assert_eq!(String::from_utf8_lossy(&gs.stdout), want, "gs disagrees");
}
