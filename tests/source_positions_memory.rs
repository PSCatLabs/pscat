//! The source-position table under the limits (#1405). The memory cap counts
//! the whole process's heap, so this is a test binary of its own, as
//! `tests/limits_memory.rs` is.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

/// A source of millions of one-line procedures is scanned in one step, and
/// the table that keeps where each was written is held to the memory cap
/// like the procedures themselves: the program stops with
/// `MemoryExceeded` instead of passing it.
#[test]
fn the_table_is_held_to_the_memory_cap() {
    let mut src = String::new();
    for i in 0..400_000 {
        src.push_str(&format!("/p{i} {{ {i} pop }} def\n"));
    }
    let mut interp = Interp::new();
    interp
        .set_limits(Limits::none().with_max_memory(16 << 20))
        .unwrap();
    assert_eq!(interp.run_str(&src), Err(PsError::MemoryExceeded));
}
