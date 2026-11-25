//! The host API's value copy under the memory cap (PR #1410 review,
//! finding 1). The cap counts the whole process's heap, so this is a test
//! binary of its own with one test, as `tests/limits_memory.rs` is.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

/// A structure that holds itself twice would copy out to billions of nodes;
/// under `--max-memory` the copy stops at the cap, uncatchably, long before
/// the node cap, and the interpreter is usable afterwards.
#[test]
fn value_copy_is_held_to_the_memory_cap() {
    let mut interp = Interp::new();
    interp
        .register_operator("extract", |i| {
            i.pop_value()?;
            Ok(())
        })
        .unwrap();
    interp
        .run_str("/a 2 array def a 0 a put a 1 a put")
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_memory(4 << 20))
        .unwrap();
    let err = interp.run_str("{ a extract } stopped").unwrap_err();
    assert_eq!(err, PsError::MemoryExceeded);
    interp.clear_operands();
    // A small value still copies under the same cap.
    interp.run_str("[1 2 3] extract").unwrap();
}
