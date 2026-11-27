//! `defineuserobject` under the memory limit (#1265). It counts the whole
//! process's heap, so this binary has exactly one test and nothing else
//! allocates alongside it (the same shape as `limits_memory.rs`).

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

/// A growth that would pass `--max-memory` is refused before the new array
/// is made, and the old array is still what `UserObjects` holds.
#[test]
fn a_growth_past_the_memory_limit_is_refused_before_it_is_made() {
    assert!(pscat::memtrack::installed());
    let slot = std::mem::size_of::<pscat::Object>();
    let mut it = Interp::new();
    // The biggest growth: a 65535-slot array, defined at index 65535 (its
    // own length), becomes 131070 slots.
    it.run_str("userdict /UserObjects 65535 array put")
        .expect("a big array");
    // Room for the array that is there and a little more, not for a copy.
    let cap = 65535 * slot + (1 << 20);
    it.set_limits(Limits::none().with_max_memory(cap))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("65535 (x) defineuserobject"),
        Err(PsError::MemoryExceeded)
    );
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str("userdict /UserObjects get length = 65534 execuserobject type =")
        .expect("the array is intact");
    assert_eq!(
        String::from_utf8_lossy(it.captured_stdout()),
        "65535\nnulltype\n"
    );

    // The snapshot a first write inside a `save` takes is a copy of the
    // whole array too, and is refused the same way, for both operators.
    for write in ["0 (x) defineuserobject", "0 undefineuserobject"] {
        let mut it = Interp::new();
        it.run_str("userdict /UserObjects 65535 array put 0 (keep) defineuserobject")
            .expect("a big array");
        it.set_limits(Limits::none().with_max_memory(65535 * slot / 2))
            .expect("the allocator is installed");
        assert_eq!(
            it.run_str(&format!("save {write}")),
            Err(PsError::MemoryExceeded),
            "{write}"
        );
        it.set_limits(Limits::none()).expect("no limit");
        it.capture_stdout();
        it.run_str("0 execuserobject ==").expect("slot 0 is intact");
        assert_eq!(String::from_utf8_lossy(it.captured_stdout()), "(keep)\n");
    }

    // The same growth fits under a limit with room for it.
    let mut it = Interp::new();
    it.run_str("userdict /UserObjects 65535 array put")
        .expect("a big array");
    it.set_limits(Limits::none().with_max_memory(65535 * slot * 8 + (8 << 20)))
        .expect("the allocator is installed");
    it.run_str("65535 (x) defineuserobject 65535 execuserobject type pop")
        .expect("grows within the limit");
}
