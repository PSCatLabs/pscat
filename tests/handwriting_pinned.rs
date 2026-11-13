//! `Library::Embedded` pins the compiled-in handwriting font (issue #1182):
//! one choice, made on `HandOptions::library`, covers `render_source`,
//! `measure_source` and the glyph checks, whatever `$PSCAT_LIB`, `$PSCAT_HOME`
//! and the working directory hold. `Library::Resolved` (the default) still
//! follows them, in that order.
//!
//! One test function, in its own test binary, because it sets `PSCAT_LIB`,
//! `PSCAT_HOME` and the process's current directory, and the resolver reads the
//! environment once, on first use: nothing else here can race it, and it sets
//! them before pscat first looks. (pscat-mcp's worker sees a `lib/handscript.ps`
//! in its font-pack `$PSCAT_HOME`, and one under a working directory of `/`.)

use pscat::Interp;
use pscat::handwriting::{
    HandOptions, Library, drawable_in, has_glyph, has_glyph_in, measure_source, parse_line_count,
    render_source,
};

mod common;

const NAME: &str = "lib/handscript.ps";
const PERIOD_DEF: &str = "/period     [200 [60 25 75 15 65 5]] def";
const COMMA_DEF: &str = "/comma      [220 [70 40 60 -10 20 -90]] def";
const HYPHEN_DEF: &str = "/hyphen     [320 [40 250 240 260]] def";

/// The embedded font with one glyph removed and a marker comment on top, which
/// is still a working font for discovery and for the renderer.
fn without(embedded: &str, glyph_def: &str, replacement: &str, marker: &str) -> String {
    assert!(
        embedded.contains(glyph_def),
        "{glyph_def} is where the test expects"
    );
    format!("% {marker}\n{}", embedded.replace(glyph_def, replacement))
}

#[test]
fn the_embedded_library_ignores_pscat_lib_home_and_the_working_directory() {
    let embedded = include_str!("../lib/handscript.ps");
    let home = common::scratch_dir("hw-pinned-home");
    let cwd = common::scratch_dir("hw-pinned-cwd");
    // $PSCAT_LIB names the lib directory itself (it is joined with what follows
    // "lib/"), so the file is <dir>/handscript.ps, not <dir>/lib/handscript.ps.
    let libenv = common::scratch_dir("hw-pinned-lib");
    for dir in [&home, &cwd] {
        std::fs::create_dir_all(dir.join("lib")).expect("scratch lib dir");
    }
    let lib_file = libenv.join("handscript.ps");
    let home_file = home.join(NAME);
    let cwd_file = cwd.join(NAME);
    std::fs::write(
        &lib_file,
        without(
            embedded,
            HYPHEN_DEF,
            "/hyphen [320] def",
            "lib-override-marker",
        ),
    )
    .unwrap();
    std::fs::write(
        &home_file,
        without(
            embedded,
            PERIOD_DEF,
            "/period [200] def",
            "home-override-marker",
        ),
    )
    .unwrap();
    std::fs::write(
        &cwd_file,
        without(
            embedded,
            COMMA_DEF,
            "/comma [220] def",
            "cwd-override-marker",
        ),
    )
    .unwrap();

    let original_dir = std::env::current_dir().expect("a current directory");
    // SAFETY: the only test in this binary, and nothing has read the
    // environment or the resolver's roots yet (they are read on first use).
    unsafe {
        std::env::remove_var("PSCAT_ROOT");
        std::env::set_var("PSCAT_LIB", &libenv);
        std::env::set_var("PSCAT_HOME", &home);
        // The per-user data directory is under these; point it at an empty
        // place so an installed pscat on this machine can not change phase 4.
        std::env::set_var("HOME", &cwd);
        std::env::set_var("XDG_DATA_HOME", &cwd);
        std::env::set_var("APPDATA", &cwd);
    }
    std::env::set_current_dir(&cwd).expect("chdir to the scratch directory");

    let mut pinned = HandOptions::new("a.,-");
    pinned.library = Library::Embedded;
    let resolved = HandOptions::new("a.,-");
    assert_eq!(
        resolved.library,
        Library::Resolved,
        "the default is today's behaviour"
    );

    // The pinned programs carry exactly the compiled-in font, in either phase.
    let check_pinned = |phase: &str| {
        for src in [measure_source(&pinned), render_source(&pinned, 100)] {
            assert!(
                src.starts_with(embedded),
                "{phase}: not the compiled-in font"
            );
            assert!(
                ![
                    "lib-override-marker",
                    "home-override-marker",
                    "cwd-override-marker"
                ]
                .iter()
                .any(|m| src.contains(m)),
                "{phase}: an override leaked into the pinned program"
            );
        }
        for (c, want) in [
            ('a', true),
            ('A', true),
            ('.', true),
            (',', true),
            ('-', true),
            (' ', false),
        ] {
            assert_eq!(has_glyph_in(Library::Embedded, c), want, "{phase}: {c:?}");
        }
        for only in [".", ",", "-"] {
            assert!(drawable_in(pinned.library, only), "{phase}: {only:?}");
        }
        assert!(!drawable_in(pinned.library, " "));
        // And the pinned measuring program runs and counts lines.
        let mut interp = Interp::new();
        interp.capture_stdout();
        interp
            .run_str(&measure_source(&pinned))
            .expect("the pinned program runs");
        let out = String::from_utf8_lossy(interp.captured_stdout()).into_owned();
        assert_eq!(parse_line_count(&out), Some(1), "{phase}");
    };

    // Phase 1: all three overrides exist; $PSCAT_LIB comes first in the search.
    assert!(
        measure_source(&resolved).contains("lib-override-marker"),
        "PSCAT_LIB not honoured"
    );
    assert!(render_source(&resolved, 100).contains("lib-override-marker"));
    assert!(
        !has_glyph('-') && has_glyph('.') && has_glyph(','),
        "Resolved follows the PSCAT_LIB override"
    );
    assert!(!drawable_in(Library::Resolved, "-") && drawable_in(Library::Resolved, ".,"));
    check_pinned("with all three overrides");

    // Phase 2: $PSCAT_LIB's file is gone; $PSCAT_HOME comes next.
    std::fs::remove_file(&lib_file).unwrap();
    assert!(
        measure_source(&resolved).contains("home-override-marker"),
        "PSCAT_HOME not honoured"
    );
    assert!(render_source(&resolved, 100).contains("home-override-marker"));
    assert!(
        !has_glyph('.') && has_glyph(',') && has_glyph('-'),
        "Resolved follows the home override"
    );
    assert!(!drawable_in(Library::Resolved, ".") && drawable_in(Library::Resolved, ",a"));
    check_pinned("with the home and working-directory overrides");

    // Phase 3: only the working directory's override is left.
    std::fs::remove_file(&home_file).unwrap();
    assert!(
        measure_source(&resolved).contains("cwd-override-marker"),
        "cwd not consulted"
    );
    assert!(render_source(&resolved, 100).contains("cwd-override-marker"));
    assert!(
        has_glyph('.') && !has_glyph(',') && has_glyph('-'),
        "Resolved follows the cwd override"
    );
    assert!(!drawable_in(Library::Resolved, ",") && drawable_in(Library::Resolved, "."));
    check_pinned("with only the working-directory override");

    // Phase 4: no override anywhere; the two choices agree.
    std::fs::remove_file(&cwd_file).unwrap();
    assert!(measure_source(&resolved).starts_with(embedded));
    assert!(has_glyph('.') && has_glyph(',') && has_glyph('-'));
    assert!(has_glyph_in(Library::Embedded, '.'));
    check_pinned("with no override");

    std::env::set_current_dir(original_dir).expect("restore the directory");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&libenv);
}
