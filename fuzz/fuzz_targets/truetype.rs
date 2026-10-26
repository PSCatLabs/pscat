#![no_main]
//! A font file (TrueType, OpenType/CFF, a collection, a variable font) through the loader and every glyph outline.
//! Run: `cargo +nightly fuzz run truetype`.

use libfuzzer_sys::fuzz_target;

// Counts the heap so the harness's memory limit applies (the same allocator
// the `pscat` binary uses).
#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

fuzz_target!(
    init: pscat::fuzzing::tolerate_dependency_panics(),
    |data: &[u8]| pscat::fuzzing::truetype(data)
);
