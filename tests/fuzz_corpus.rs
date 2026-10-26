//! Replays the fuzz seed corpus and the crash regressions through the fuzz
//! entry points on every `cargo test` (issue #604), so "no panics on the
//! corpus" is gated in ordinary CI without a nightly toolchain or libFuzzer.
//!
//! `fuzz/seeds/<target>/` holds the committed seeds; a crash a fuzz run finds
//! is fixed and its input committed to `fuzz/regressions/<target>/`, which
//! this test then keeps replaying.
//!
//! Every replay runs on a worker thread under a wall-clock deadline (issue
//! #704): the entry points apply the interpreter limits themselves, but an
//! input that hangs anyway (a seed that reads `/dev/zero`, a parser loop no
//! limit sees) fails the test in thirty seconds, naming the file, and does
//! not stall CI until the job's own timeout.

use std::path::{Path, PathBuf};

mod common;

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

/// Wall-clock allowance for one input. The fuzz limits stop an interpreter
/// run after two seconds; this is the margin for a slow CI machine.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// Run `entry` on `data` on a worker thread; panic if it panics or does not
/// return within [`DEADLINE`]. A hung worker is abandoned (the test process
/// exits when the tests are done).
fn replay(name: &str, data: &[u8], entry: fn(&[u8])) {
    let (tx, rx) = std::sync::mpsc::channel();
    let data = data.to_vec();
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || {
            entry(&data);
            let _ = tx.send(());
        })
        .expect("spawn replay thread");
    match rx.recv_timeout(DEADLINE) {
        Ok(()) => {}
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!("{name}: no result after {DEADLINE:?} (a hang)")
        }
        // The sender was dropped without sending: the worker panicked.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{name}: panicked"),
    }
}

/// Replay every committed input of `target` through `entry`.
fn replay_all(target: &str, min_seeds: usize, entry: fn(&[u8])) {
    let files = inputs(target);
    assert!(
        files.len() >= min_seeds,
        "{target}: seed corpus went missing: {files:?}"
    );
    for path in files {
        let data = std::fs::read(&path).expect("read seed");
        replay(&path.display().to_string(), &data, entry);
    }
}

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
    replay_all("lexer", 10, pscat::fuzzing::lexer);
}

#[test]
fn interpreter_seeds_do_not_panic() {
    replay_all("interp", 20, pscat::fuzzing::interpreter);
}

#[test]
fn truetype_seeds_do_not_panic() {
    replay_all("truetype", 4, pscat::fuzzing::truetype);
}

#[test]
fn type1_charstring_seeds_do_not_panic() {
    replay_all("type1_charstring", 8, pscat::fuzzing::type1_charstring);
}

#[test]
fn type1_font_seeds_do_not_panic() {
    replay_all("type1_font", 4, pscat::fuzzing::type1_font);
}

#[test]
fn filter_seeds_do_not_panic() {
    replay_all("filters", 8, pscat::fuzzing::filters);
}

#[test]
fn image_seeds_do_not_panic() {
    replay_all("image", 15, pscat::fuzzing::image);
}

/// Every new target's seeds also survive truncation and a flipped byte at a
/// dozen places: the cheap, deterministic cousin of what the fuzzer does.
#[test]
fn new_targets_survive_truncated_and_mutated_seeds() {
    type Entry = fn(&[u8]);
    let targets: [(&str, Entry); 5] = [
        ("truetype", pscat::fuzzing::truetype),
        ("type1_charstring", pscat::fuzzing::type1_charstring),
        ("type1_font", pscat::fuzzing::type1_font),
        ("filters", pscat::fuzzing::filters),
        ("image", pscat::fuzzing::image),
    ];
    for (target, entry) in targets {
        for path in inputs(target) {
            let data = std::fs::read(&path).expect("read seed");
            let name = path.display().to_string();
            let step = (data.len() / 12).max(1);
            for cut in (0..data.len()).step_by(step) {
                replay(&format!("{name} cut at {cut}"), &data[..cut], entry);
            }
            for at in (0..data.len()).step_by(step) {
                let mut mutated = data.clone();
                mutated[at] ^= 0x5a;
                replay(&format!("{name} flipped at {at}"), &mutated, entry);
            }
        }
    }
}

/// The harness names no read root, so `(/dev/zero) (r) file` is allowed to
/// start, and the fuzz limits are what end it (issue #702 made the
/// whole-file read look at them as the bytes arrive). The seed must end on a
/// limit, not on a refused path: otherwise it would prove nothing.
#[test]
fn a_dev_zero_seed_is_stopped_by_a_limit() {
    let seed = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/seeds/interp/read_dev_zero.seed"),
    )
    .expect("the /dev/zero seed");
    let result = pscat::fuzzing::interpreter_result(&seed);
    assert!(
        matches!(&result, Err(e) if e.limit().is_some()),
        "expected a limit error, got {result:?}"
    );
}

/// A FIFO nobody writes to blocks the open itself; a seed cannot be a FIFO,
/// so the test makes one. The time limit has to end it.
#[cfg(unix)]
#[test]
fn a_fifo_nobody_writes_to_is_stopped_by_a_limit() {
    let dir = common::scratch_dir("fuzz-fifo");
    let fifo = dir.join("stuck.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo")
            .success()
    );
    let src = format!("({}) (r) file pop", common::ps_path(&fifo));
    let started = std::time::Instant::now();
    let result = pscat::fuzzing::interpreter_result(src.as_bytes());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        matches!(&result, Err(e) if e.limit().is_some()),
        "expected a limit error, got {result:?}"
    );
    assert!(started.elapsed() < DEADLINE, "the limit came too late");
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
