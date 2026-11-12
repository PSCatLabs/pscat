//! The shared Ghostscript skip rule (issue #985): `common::gs_decision`.

mod common;

#[test]
fn gs_present_runs_the_check_whether_or_not_it_is_required() {
    assert_eq!(common::gs_decision(true, false), Ok(true));
    assert_eq!(common::gs_decision(true, true), Ok(true));
}

#[test]
fn gs_missing_skips_when_not_required() {
    assert_eq!(common::gs_decision(false, false), Ok(false));
}

#[test]
fn gs_missing_is_an_error_when_required() {
    let why = common::gs_decision(false, true).expect_err("required gs must not skip");
    assert!(why.contains("PSCAT_REQUIRE_GS"), "{why}");
}
