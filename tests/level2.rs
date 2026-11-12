//! packedarray, packing, clocks, languagelevel, resources — Stage 8
//! task 5. The fictions (plain arrays, 0 0 resource status) are
//! deliberate and documented in ops/level2.rs.

use pscat::{Interp, PsError};

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
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "0");
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
    assert_eq!(top_repr(&mut it), "0");
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
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "0");
    assert_eq!(top_repr(&mut it), "false", "post-save resource rolled back");
}
