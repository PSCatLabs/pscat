#![no_main]
//! The whole interpreter on arbitrary bytes, under tight execution limits.
//! Run: `cargo +nightly fuzz run interp`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(|data: &[u8]| pscat::fuzzing::interpreter(data));
