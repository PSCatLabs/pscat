#![no_main]
//! The decode filters (ASCIIHex, ASCII85, RunLength, eexec, Flate, LZW, DCT), singly and chained.
//! Run: `cargo +nightly fuzz run filters`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(
    init: pscat::fuzzing::tolerate_dependency_panics(),
    |data: &[u8]| pscat::fuzzing::filters(data)
);
