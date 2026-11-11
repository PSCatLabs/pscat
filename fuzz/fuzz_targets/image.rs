#![no_main]
//! The image operators: a PostScript program starting with %!, or a recipe that builds an image, colorimage, dictionary or ImageType 3 call.
//! Run: `cargo +nightly fuzz run image`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(
    init: pscat::fuzzing::tolerate_dependency_panics(),
    |data: &[u8]| pscat::fuzzing::image(data)
);
