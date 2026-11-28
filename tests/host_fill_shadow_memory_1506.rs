//! `Interp::drop_fill_shadow` against `--max-memory` (#1506). The memory limit
//! counts the whole process's heap, so this is a test binary of its own with
//! one test, as `tests/groups_memory.rs` is.

use pscat::{Interp, Limits};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

/// The call neither allocates nor fails under a cap, and the heap it leaves
/// is the heap the shadow held less: it frees the shadow. The pixels at the
/// call are the pixels after it.
#[test]
fn the_drop_allocates_nothing_and_gives_the_shadow_back() {
    let mut it = Interp::with_page(1000, 1000).expect("page");
    it.set_limits(Limits::none().with_max_memory(200 * MIB))
        .expect("the allocator is installed");
    // Translucent fills allocate the shadow's rows: 8 bytes a pixel, 8 MB.
    it.run_str("0 setgray 0 0 1000 1000 rectfill 1 setgray 0.02 setalpha 0 0 1000 1000 rectfill")
        .expect("runs");
    let pixels = it.canvas_premultiplied_rgba8().to_vec();
    let held = pscat::memtrack::live_bytes();
    assert!(it.drop_fill_shadow());
    let after = pscat::memtrack::live_bytes();
    assert_eq!(it.canvas_premultiplied_rgba8(), &pixels[..]);
    let freed = held.saturating_sub(after);
    assert!(
        (7 * MIB..=9 * MIB).contains(&freed),
        "the shadow is 8 bytes a pixel: freed {freed}"
    );
    // Again: nothing more to free, nothing allocated.
    assert!(it.drop_fill_shadow());
    assert_eq!(pscat::memtrack::live_bytes(), after);
    // Under a cap that the shadow's page would not fit: the call still works.
    let mut it = Interp::with_page(1000, 1000).expect("page");
    it.set_limits(Limits::none().with_max_memory(20 * MIB))
        .expect("the allocator is installed");
    it.run_str("0 setgray 0 0 1000 1000 rectfill 1 setgray 0.02 setalpha 0 0 1000 1000 rectfill")
        .expect("runs");
    assert!(it.drop_fill_shadow());
}
