#![no_main]
//! One Type 1 charstring plus its subroutines, straight into the charstring interpreter.
//! Run: `cargo +nightly fuzz run type1_charstring`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(|data: &[u8]| pscat::fuzzing::type1_charstring(data));
