//! A transparency group's layer (#1406) is asked of `--max-memory` before
//! it is made. The memory limit counts the whole process's heap, so this is
//! a test binary of its own with one test, as `tests/limits_memory.rs` is.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

#[test]
fn a_layer_past_the_memory_cap_is_refused_before_it_is_made() {
    // A 2000 x 2000 page: each layer is 16 MiB.
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(Limits::none().with_max_memory(40 * MIB))
        .expect("the allocator is installed");
    // One fits; three nested do not, and the third is refused before its
    // 16 MiB is allocated.
    assert!(it.run_str("<< >> begingroup endgroup").is_ok());
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(Limits::none().with_max_memory(40 * MIB))
        .expect("the allocator is installed");
    let before = pscat::memtrack::live_bytes();
    assert_eq!(
        it.run_str("<< >> begingroup << >> begingroup << >> begingroup"),
        Err(PsError::MemoryExceeded)
    );
    // Two layers at most were made, and the run's groups were ended.
    assert!(pscat::memtrack::live_bytes() < before + 34 * MIB);
    drop(it);

    // Codex round 1, finding 1: closing a group under `--pdf` stores its
    // content stream (about 47 bytes a fill: 400 000 fills are 19 MB) as a
    // form, a copy asked of the cap first. Recording the stream takes the
    // heap to about 33 MiB here (its capacity), so the copy would pass a
    // 40 MiB cap: refused, by `endgroup` and by the end of the program.
    for src in [
        "<< >> begingroup 400000 { 0 0 1 1 rectfill } repeat endgroup",
        "<< >> begingroup 400000 { 0 0 1 1 rectfill } repeat",
    ] {
        let mut it = Interp::with_page(100, 100).expect("page");
        it.gfx_mut().enable_pdf();
        it.set_limits(Limits::none().with_max_memory(40 * MIB))
            .expect("the allocator is installed");
        assert_eq!(it.run_str(src), Err(PsError::MemoryExceeded), "{src}");
    }
    // A group a quarter the size fits.
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_pdf();
    it.set_limits(Limits::none().with_max_memory(40 * MIB))
        .expect("the allocator is installed");
    assert!(
        it.run_str("<< >> begingroup 100000 { 0 0 1 1 rectfill } repeat endgroup")
            .is_ok()
    );
}
