//! The resource resolver end to end through the real binary (issue
//! #598): `src/paths.rs`'s unit tests pin the order with injected
//! roots; these check the environment variables and the trace line
//! the clean-install test relies on actually reach it.

mod common;

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn pscat_in(dir: &std::path::Path) -> Command {
    let mut c = Command::new(BIN);
    c.current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env("PSCAT_TRACE_RESOURCES", "1");
    c
}

#[test]
fn pscat_lib_overrides_the_embedded_copy() {
    let dir = common::scratch_dir("resources-pscat-lib");
    let lib = dir.join("mylib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("artkit.ps"), "/OVERRIDE (pscat-lib) def\n").unwrap();
    let out = pscat_in(&dir)
        .env("PSCAT_LIB", &lib)
        .args(["-e", "(lib/artkit.ps) run OVERRIDE ="])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("pscat-lib"), "stdout: {stdout}");
    assert!(
        stderr.contains("pscat: resource lib/artkit.ps <- pscat-lib ("),
        "stderr: {stderr}"
    );
}

#[test]
fn a_lib_dir_beside_the_program_overrides_the_embedded_copy() {
    let dir = common::scratch_dir("resources-cwd-lib");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/artkit.ps"), "/OVERRIDE (cwd) def\n").unwrap();
    let out = pscat_in(&dir)
        .args(["-e", "(lib/artkit.ps) run OVERRIDE ="])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cwd"));
    assert!(
        stderr.contains("pscat: resource lib/artkit.ps <- cwd ("),
        "{stderr}"
    );
}

#[test]
fn with_no_lib_on_disk_the_embedded_copy_serves() {
    // A scratch dir has no lib/; with PSCAT_* unset the only disk
    // places left are the user data dir and the exe dir (target/ci/,
    // which has no lib/), so the embedded copy answers before the
    // checkout heuristics would.
    let dir = common::scratch_dir("resources-embedded");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let out = pscat_in(&dir)
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .env_remove("APPDATA")
        .args(["-e", "(lib/artkit.ps) run (lib/fonts/neon.ps) run"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("pscat: resource lib/artkit.ps <- embedded\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("pscat: resource lib/fonts/neon.ps <- embedded\n"),
        "{stderr}"
    );
}

#[test]
fn a_missing_file_is_still_undefinedfilename() {
    let dir = common::scratch_dir("resources-missing");
    let out = pscat_in(&dir)
        .args([
            "-e",
            "{ (lib/no_such_lib.ps) run } stopped { $error /errorname get = } if",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("undefinedfilename"), "stdout: {stdout}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("lib/no_such_lib.ps <- not found"));
}
