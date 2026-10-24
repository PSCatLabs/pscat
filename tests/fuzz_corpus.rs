//! Replays the fuzz seed corpus and the crash regressions through the fuzz
//! entry points on every `cargo test` (issue #604), so "no panics on the
//! corpus" is gated in ordinary CI without a nightly toolchain or libFuzzer.
//!
//! `fuzz/seeds/<target>/` holds the committed seeds; a crash a fuzz run finds
//! is fixed and its input committed to `fuzz/regressions/<target>/`, which
//! this test then keeps replaying.

use std::path::{Path, PathBuf};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fn inputs(target: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz");
    let mut files = Vec::new();
    for kind in ["seeds", "regressions"] {
        let dir = root.join(kind).join(target);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n != ".gitkeep") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn lexer_seeds_do_not_panic() {
    let files = inputs("lexer");
    assert!(files.len() >= 10, "seed corpus went missing: {files:?}");
    for path in files {
        let data = std::fs::read(&path).expect("read seed");
        pscat::fuzzing::lexer(&data);
    }
}

#[test]
fn interpreter_seeds_do_not_panic() {
    let files = inputs("interp");
    assert!(files.len() >= 20, "seed corpus went missing: {files:?}");
    for path in files {
        let data = std::fs::read(&path).expect("read seed");
        pscat::fuzzing::interpreter(&data);
    }
}

/// The seeds also survive being cut off anywhere and mangled: the cheap,
/// deterministic cousin of what the fuzzer does.
#[test]
fn truncated_and_mutated_seeds_do_not_panic() {
    for path in inputs("interp").into_iter().take(40) {
        let data = std::fs::read(&path).expect("read seed");
        let step = (data.len() / 12).max(1);
        for cut in (0..data.len()).step_by(step) {
            pscat::fuzzing::lexer(&data[..cut]);
            pscat::fuzzing::interpreter(&data[..cut]);
        }
        for at in (0..data.len()).step_by(step) {
            let mut mutated = data.clone();
            mutated[at] ^= 0x5a;
            pscat::fuzzing::interpreter(&mutated);
        }
    }
}
