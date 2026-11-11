#![no_main]
//! A complete Type 1 font program (eexec, RD, definefont) run through the interpreter, then text shown in every font it defined.
//! Run: `cargo +nightly fuzz run type1_font`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(
    init: pscat::fuzzing::tolerate_dependency_panics(),
    |data: &[u8]| pscat::fuzzing::type1_font(data)
);
