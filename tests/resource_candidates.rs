//! Issue #1177: what a disk candidate of the resource resolver may be.
//!
//! `paths::open`, `run` and `file` read a relative name through a search
//! order that includes `$PSCAT_HOME` and the current directory, so a FIFO
//! with no writer, or a multi-GB file, sitting at one of those places must
//! not block the resolver or exhaust memory. A non-regular file is "not
//! here" and the search goes on; an oversized one is refused.
//!
//! The resolver reads `$PSCAT_HOME` once per process, so this binary holds a
//! single test that sets it before the first resolver call. The test's
//! working directory is the crate root, whose own `lib/` is the next place
//! after `$PSCAT_HOME` in the search order, so what a skipped candidate
//! falls back to is that file's bytes (or, for a name with no copy there,
//! nothing).
#![cfg(unix)]

mod common;

use std::fs::File;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use pscat::paths::{self, MAX_LIB_BYTES, MAX_RESOURCE_BYTES};
use pscat::{Interp, PsError};

fn mkfifo(path: &Path) {
    assert!(
        Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("mkfifo")
            .success()
    );
}

fn sparse(path: &Path, len: u64) {
    File::create(path).unwrap().set_len(len).unwrap();
}

/// Run `f` on a thread; a regression is a blocked open, which would hang the
/// test binary, so give up after a deadline instead.
fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("the resolver blocked on a non-regular file")
}

#[test]
fn a_fifo_or_huge_file_in_the_search_order_is_skipped() {
    let home = common::scratch_dir("resource-candidates");
    std::fs::create_dir_all(home.join("lib")).unwrap();
    std::fs::create_dir_all(home.join("fonts/catalog/Big")).unwrap();
    // SAFETY: the only test in this binary, and no resolver call has run.
    unsafe { std::env::set_var("PSCAT_HOME", &home) };

    let real = std::fs::read("lib/handscript.ps").expect("run from the crate root");

    // A FIFO where the library should be: the next place serves it.
    mkfifo(&home.join("lib/handscript.ps"));
    let found = within(|| paths::open("lib/handscript.ps")).expect("falls through");
    assert_ne!(found.backend, paths::Backend::Home);
    assert_eq!(&found.bytes[..], &real[..]);

    // `handwriting`'s own source, which resolves through the same place.
    let source = within(|| pscat::handwriting::measure_source(&Default::default()));
    assert!(source.starts_with(&String::from_utf8_lossy(&real).into_owned()));

    // `run` and `file` behave the same, with no execution limits set.
    let ok = within(|| {
        let mut it = Interp::new();
        (
            it.run_str("(lib/handscript.ps) (r) file pop"),
            it.run_str("(lib/handscript.ps) status pop pop pop pop"),
        )
    });
    assert_eq!(ok, (Ok(()), Ok(())));

    // A FIFO under a name nothing else serves is simply not found.
    mkfifo(&home.join("lib/only_a_fifo.ps"));
    assert!(within(|| paths::open("lib/only_a_fifo.ps")).is_none());
    let err = within(|| Interp::new().run_str("(lib/only_a_fifo.ps) run"));
    assert_eq!(err, Err(PsError::UndefinedFilename));

    // A directory is skipped too (it never could be read).
    std::fs::create_dir(home.join("lib/a_dir.ps")).unwrap();
    assert!(within(|| paths::open("lib/a_dir.ps")).is_none());

    // A `lib/` file over the cap is refused, not read.
    sparse(&home.join("lib/huge.ps"), MAX_LIB_BYTES + 1);
    assert!(within(|| paths::open("lib/huge.ps")).is_none());
    assert_eq!(
        within(|| Interp::new().run_str("(lib/huge.ps) run")),
        Err(PsError::UndefinedFilename)
    );
    // ... and a regular file at the cap's edge is read as before.
    sparse(&home.join("lib/edge.ps"), 4 << 20);
    let edge = within(|| paths::open("lib/edge.ps")).expect("a 4 MiB library");
    assert_eq!(edge.bytes.len(), 4 << 20);
    assert_eq!(edge.backend, paths::Backend::Home);

    // Fonts are not refused by the cap: a .ttf far bigger than any in the
    // catalog (10 MB) still resolves, while a data file over the generic cap
    // does not.
    let font = home.join("fonts/catalog/Big/Big-Regular.ttf");
    sparse(&font, 48 << 20);
    let name = "fonts/catalog/Big/Big-Regular.ttf";
    let got = within(move || paths::open(name)).expect("a 48 MiB font");
    assert_eq!(got.bytes.len(), 48 << 20);
    sparse(&home.join("data.bin"), MAX_RESOURCE_BYTES + 1);
    assert!(within(|| paths::open("data.bin")).is_none());

    let _ = std::fs::remove_dir_all(&home);
}
