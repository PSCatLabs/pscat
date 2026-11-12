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

#[test]
fn log_keys_keep_same_named_tests_in_different_suites_apart() {
    use std::path::Path;
    let name = "ghostscript_prints_the_pinned_lines";
    let a = common::gs_log_key(Some(Path::new("/t/deps/clipsave-1a2b3c4d5e6f7a8b")), name);
    let b = common::gs_log_key(Some(Path::new("/t/deps/grestore_boundary-0f9e8d7c")), name);
    assert_eq!(a, "clipsave::ghostscript_prints_the_pinned_lines");
    assert_eq!(b, "grestore_boundary::ghostscript_prints_the_pinned_lines");
    assert_ne!(a, b);
    // Windows adds `.exe`; `file_stem` drops it before the hash.
    let w = common::gs_log_key(Some(Path::new("C:/t/deps/clipsave-1a2b3c.exe")), name);
    assert_eq!(w, "clipsave::ghostscript_prints_the_pinned_lines");
    // A name with a non-hash dash keeps it; no exe falls back clearly.
    assert_eq!(
        common::gs_log_key(Some(Path::new("/t/my-suite")), "t"),
        "my-suite::t"
    );
    assert_eq!(common::gs_log_key(None, "t"), "(unknown suite)::t");
}
