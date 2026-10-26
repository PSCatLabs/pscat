//! Where pscat lets a third-party parser panic without dying (issue #704).
//!
//! `ttf-parser` and `zune-jpeg` read untrusted bytes, and the fuzz targets
//! found inputs that make each panic: a `debug_assert!` on an offset in
//! `ttf-parser` (debug builds only), an `assert!` in `zune-jpeg`'s NEON
//! upsampler (aarch64, release too). [`catch`] runs such a call and turns a
//! panic into `None`, which the caller reports as a font that does not load
//! or an `ioerror`.
//!
//! [`active`] says whether the current thread is inside one. The fuzz
//! targets install a panic hook (`fuzzing::tolerate_dependency_panics`) that
//! lets exactly those panics through: libFuzzer's own hook aborts before any
//! unwinding, which would stop every run on a bug pscat already handles.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Run `f`; `None` if it panicked.
pub(crate) fn catch<T>(f: impl FnOnce() -> T) -> Option<T> {
    DEPTH.with(|d| d.set(d.get() + 1));
    let result = catch_unwind(AssertUnwindSafe(f));
    DEPTH.with(|d| d.set(d.get() - 1));
    result.ok()
}

/// Whether this thread is inside [`catch`].
pub(crate) fn active() -> bool {
    DEPTH.with(|d| d.get() > 0)
}
