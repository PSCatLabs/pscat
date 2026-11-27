//! A library loaded without the library its `% @requires:` line names says
//! which one is missing (issue #1455).
//!
//! Since #1225 a library procedure reaches its helpers by `//name exec`, which
//! is scanned when the library loads. A library loaded without its
//! prerequisite therefore failed while loading with the name of the first
//! helper it could not find (`undefined in pjheight`), which names a helper,
//! not the library that defines it. Each library with a `@requires` line now
//! starts, right under that line, with one check per prerequisite:
//!
//! ```text
//! /pkribbon where { pop } { (drykit needs (lib/paintkit.ps) run first) errordict /undefined get exec } ifelse
//! ```
//!
//! It raises the real `undefined` error with the sentence as the offending
//! command (so `$error /command`, `handleerror` and the CLI report all carry
//! it), defines no name and runs only at load time. The probe name is one the
//! prerequisite defines (`SENTINELS`).
//!
//! The lines are written by hand in each library, so this file is what keeps
//! them from drifting: it reads every library's `@requires` line, builds the
//! text the lines must have, and compares. A new library with a `@requires`
//! line and no check, a prerequisite added to a line without its check, a
//! reordered line, or a probe name that the prerequisite no longer defines,
//! fails here.

use pscat::Interp;
use pscat::selftest::parse_requires;

/// The name each library defines that its dependants probe for. Chosen to be
/// top-level, distinctive and public (or at least stable); the test below
/// proves each one is defined by its own library and by no earlier one.
const SENTINELS: &[(&str, &str)] = &[
    ("lib/artkit.ps", "walkpath"),
    ("lib/paintkit.ps", "pkribbon"),
    ("lib/drykit.ps", "dkstroke"),
    ("lib/hatchkit.ps", "hkline"),
    ("lib/surfacekit.ps", "sfshade"),
    ("lib/headline.ps", "hllayout"),
    ("lib/lettering.ps", "psyletter"),
    ("lib/functional.ps", "fp-map"),
    ("lib/aspect.ps", "aop-before"),
    ("lib/handscript.ps", "hs-write"),
    ("lib/strokeglyphs.ps", "sgpath"),
    ("lib/typewriter_outlines.ps", "TWOutlines"),
];

fn sentinel(path: &str) -> &'static str {
    SENTINELS
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| {
            panic!("{path} is named by a @requires line but has no entry in SENTINELS")
        })
}

struct Lib {
    /// `lib/drykit.ps`
    path: String,
    /// `drykit`
    stem: String,
    source: String,
    /// The `@requires` paths, in load order.
    requires: Vec<String>,
}

/// Every `lib/*.ps` that has a `@requires` line.
fn libs_with_requires() -> Vec<Lib> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir("lib")
        .expect("lib/")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "ps"))
        .collect();
    entries.sort();
    for p in entries {
        let bytes = std::fs::read(&p).expect("read lib");
        let path = p.to_string_lossy().replace('\\', "/");
        let requires =
            parse_requires(&bytes).unwrap_or_else(|e| panic!("{path}: bad @requires: {e}"));
        if requires.is_empty() {
            continue;
        }
        let stem = p.file_stem().expect("stem").to_string_lossy().to_string();
        out.push(Lib {
            path,
            stem,
            source: String::from_utf8(bytes).expect("lib is UTF-8"),
            requires,
        });
    }
    out
}

/// The check line for one prerequisite of `lib`.
fn expected_line(lib: &Lib, prerequisite: &str) -> String {
    format!(
        "/{} where {{ pop }} {{ ({} needs ({}) run first) errordict /undefined get exec }} ifelse",
        sentinel(prerequisite),
        lib.stem,
        prerequisite
    )
}

fn expected_lines(lib: &Lib) -> Vec<String> {
    lib.requires.iter().map(|p| expected_line(lib, p)).collect()
}

fn load(it: &mut Interp, path: &str) {
    let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
}

fn defined(it: &mut Interp, name: &str) -> bool {
    it.run_str(&format!("/{name} where {{ pop true }} {{ false }} ifelse"))
        .expect("where");
    let top = it.operand_stack().last().expect("a result").text();
    it.run_str("pop").expect("pop");
    top == "true"
}

#[test]
fn every_library_with_a_requires_line_has_exactly_the_matching_checks() {
    let libs = libs_with_requires();
    assert!(libs.len() >= 19, "found only {} libraries", libs.len());
    for lib in &libs {
        let lines: Vec<&str> = lib.source.lines().collect();
        let tag = lines
            .iter()
            .position(|l| {
                l.trim_start()
                    .trim_start_matches('%')
                    .trim_start()
                    .starts_with("@requires:")
            })
            .unwrap_or_else(|| panic!("{}: no @requires line", lib.path));
        // The explanatory comment may sit between the tag and the checks.
        let mut at = tag + 1;
        while lines.get(at).is_some_and(|l| l.starts_with('%')) {
            at += 1;
        }
        let want = expected_lines(lib);
        let got: Vec<&str> = lines[at..].iter().take(want.len()).copied().collect();
        assert_eq!(
            got,
            want.iter().map(String::as_str).collect::<Vec<_>>(),
            "{}: the lines under its @requires line must be, in order:\n{}\n",
            lib.path,
            want.join("\n")
        );
        // Nothing else in the file is a check, and no code comes first.
        let checks = lines.iter().filter(|l| l.contains(" needs (lib/")).count();
        assert_eq!(checks, want.len(), "{}: stray prerequisite check", lib.path);
        let first_code = lines
            .iter()
            .position(|l| !l.trim().is_empty() && !l.starts_with('%'))
            .expect("code");
        assert_eq!(first_code, at, "{}: code before the checks", lib.path);
    }
}

#[test]
fn each_probe_name_is_defined_by_its_own_library_and_by_no_earlier_one() {
    for (path, name) in SENTINELS {
        // The chain the library's own @requires line asks for, if any.
        let bytes = std::fs::read(path).expect("lib");
        let requires = parse_requires(&bytes).expect("requires");
        let mut it = Interp::new();
        for r in &requires {
            load(&mut it, r);
        }
        assert!(
            !defined(&mut it, name),
            "{path}: `{name}` is already defined before the library loads"
        );
        load(&mut it, path);
        assert!(defined(&mut it, name), "{path} does not define `{name}`");
    }
}

#[test]
fn loading_without_a_prerequisite_names_the_library_to_run_first() {
    for lib in libs_with_requires() {
        // Load the first k prerequisites (k = 0 is a bare interpreter); the
        // error names prerequisite k, the first one missing.
        for k in 0..lib.requires.len() {
            let mut it = Interp::new();
            for r in &lib.requires[..k] {
                load(&mut it, r);
            }
            it.run_str(&format!(
                "{{ ({}) run }} stopped {{ $error /errorname get $error /command get }} {{ null null }} ifelse",
                lib.path
            ))
            .unwrap_or_else(|e| panic!("{}: {}", lib.path, it.error_report(&e)));
            let stack = it.operand_stack();
            let (name, command) = (stack[stack.len() - 2].text(), stack[stack.len() - 1].text());
            let message = format!("{} needs ({}) run first", lib.stem, lib.requires[k]);
            assert_eq!(name, "undefined", "{} with {k} loaded", lib.path);
            assert_eq!(command, message, "{} with {k} loaded", lib.path);
        }
    }
}

#[test]
fn with_every_prerequisite_loaded_the_library_loads_and_leaves_the_stack_clean() {
    for lib in libs_with_requires() {
        let mut it = Interp::new();
        for r in &lib.requires {
            load(&mut it, r);
        }
        load(&mut it, &lib.path);
        assert!(
            it.operand_stack().is_empty(),
            "{}: the check left operands behind",
            lib.path
        );
    }
}

#[test]
fn the_cli_report_carries_the_sentence() {
    // What a program author sees: the error line names the library.
    let mut it = Interp::new();
    let e = it
        .run_str("(lib/drykit.ps) run")
        .expect_err("drykit without artkit must not load");
    let report = it.error_report(&e);
    assert!(
        report.contains("drykit needs (lib/artkit.ps) run first"),
        "{report}"
    );
}
