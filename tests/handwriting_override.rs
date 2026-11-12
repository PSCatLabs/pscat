//! `has_glyph` follows the handwriting library `render_source` would embed
//! (issue #1170): a `$PSCAT_HOME` override that changes changes the answer, and
//! one that never finishes gives "no glyphs" promptly instead of hanging.
//!
//! One test function, in its own test binary, because it sets `PSCAT_HOME`
//! for the process: nothing else here can race it, and it sets it before
//! pscat first looks.

use pscat::handwriting::{HandOptions, drawable, has_glyph, render_source};

#[test]
fn an_override_library_is_followed_bounded_and_recoverable() {
    let home = std::env::temp_dir().join(format!("pscat-hw-override-{}", std::process::id()));
    let lib = home.join("lib");
    std::fs::create_dir_all(&lib).expect("scratch lib dir");
    let file = lib.join("handscript.ps");
    // SAFETY: the only test in this binary, and nothing has read the
    // environment yet (the roots are read on first use).
    unsafe { std::env::set_var("PSCAT_HOME", &home) };

    let embedded = include_str!("../lib/handscript.ps");
    let opts = HandOptions::new("a.");
    let uses_override = |marker: &str| render_source(&opts, 100).contains(marker);

    // A runaway override: no glyphs, quickly, and no panic.
    std::fs::write(&file, "% runaway override\n{ } loop\n").unwrap();
    assert!(uses_override("runaway override"), "PSCAT_HOME not honoured");
    let started = std::time::Instant::now();
    assert!(!has_glyph('a') && !drawable("abc"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "discovery was not bounded"
    );

    // A good override afterwards works: the failure was not remembered.
    std::fs::write(&file, embedded).unwrap();
    assert!(has_glyph('a') && has_glyph('.') && !has_glyph(' '));

    // The override changes: the answer changes with it, as render_source's does.
    let no_period = embedded.replace(
        "/period     [200 [60 25 75 15 65 5]] def",
        "/period [200] def",
    );
    assert_ne!(no_period, embedded);
    std::fs::write(&file, &no_period).unwrap();
    assert!(!has_glyph('.') && has_glyph('a') && !drawable("..."));
    std::fs::write(&file, embedded).unwrap();
    assert!(has_glyph('.') && drawable("..."));

    let _ = std::fs::remove_dir_all(&home);
}
