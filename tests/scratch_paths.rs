//! Issue #487: no test may name a shared temp path.
//!
//! Agents run `cargo test` in several worktrees at once. A test that
//! writes `temp_dir()/some_fixed_name.png` then races its twin in the
//! other run (`read gs output: NotFound`). Every scratch path comes from
//! `common::scratch_dir`, which is unique per call and per process. This
//! test keeps it that way: it fails on any other route to the system temp
//! dir in a test file.

mod common;

#[test]
fn tests_take_scratch_paths_only_from_the_shared_helper() {
    // Built from pieces so this file does not match itself.
    let needles = [
        ["temp", "_dir("].concat(),
        ["\"/", "tmp"].concat(),
        ["TMP", "DIR"].concat(),
    ];
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "scratch_paths.rs" {
            continue;
        }
        for (i, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if needles.iter().any(|n| line.contains(n.as_str())) {
                offenders.push(format!("tests/{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use common::scratch_dir(tag) instead of the system temp dir:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn scratch_dirs_are_distinct_and_exist() {
    let a = common::scratch_dir("probe");
    let b = common::scratch_dir("probe");
    assert_ne!(a, b);
    assert!(a.is_dir() && b.is_dir());
    assert!(
        a.to_string_lossy()
            .contains(&std::process::id().to_string())
    );
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
}
