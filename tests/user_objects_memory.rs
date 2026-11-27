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

    refusals_leave_the_state_alone(slot);
}

/// What a refusal left: the operand stack as `count`, the top object's
/// type, and `UserObjects`' length (0 for none), read with no limit.
fn state_after(it: &mut Interp) -> (usize, String, usize) {
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str(
        "count 8 string cvs print ( ) print count 0 gt { dup type 16 string cvs print } { (-) print } ifelse \
         ( ) print userdict /UserObjects known { userdict /UserObjects get length } { 0 } ifelse \
         8 string cvs print",
    )
    .expect("the interpreter still runs");
    let out = String::from_utf8_lossy(it.captured_stdout()).into_owned();
    let mut words = out.split_whitespace();
    let count = words.next().and_then(|w| w.parse().ok()).expect("count");
    let top = words.next().expect("top").to_string();
    let len = words.next().and_then(|w| w.parse().ok()).expect("length");
    (count, top, len)
}

/// Codex round 1 (L1, L2, L3, L4): each refusal comes before anything is
/// made or installed, so the operands and `UserObjects` are as they were.
fn refusals_leave_the_state_alone(slot: usize) {
    // L1: a one-slot view over a 200000-slot array; the save snapshot
    // copies all of it, which is what is refused.
    for (write, operands) in [("0 (new) defineuserobject", 3), ("0 undefineuserobject", 2)] {
        let mut it = Interp::new();
        it.run_str("/big 200000 array def userdict /UserObjects big 0 1 getinterval put")
            .expect("a view");
        it.set_limits(Limits::none().with_max_memory(200000 * slot / 2))
            .expect("the allocator is installed");
        assert_eq!(
            it.run_str(&format!("save {write}")),
            Err(PsError::MemoryExceeded),
            "L1 {write}"
        );
        let (count, _, len) = state_after(&mut it);
        assert_eq!((count, len), (operands, 1), "L1 {write}");
    }

    // L2: the snapshot is taken once per save level, so a second write
    // needs no more room than the first.
    let mut it = Interp::new();
    it.run_str("65535 null defineuserobject").expect("an array");
    it.set_limits(Limits::none().with_max_memory(65535 * slot * 3 / 2))
        .expect("the allocator is installed");
    it.run_str("save 0 null defineuserobject 0 (new) defineuserobject 0 undefineuserobject")
        .expect("one snapshot fits, and a second is not taken");

    // L3: the first write to a big userdict inside a save copies it.
    let mut it = Interp::new();
    it.run_str("1 1 100000 { 20 string cvs cvn 0 def } for")
        .expect("a big userdict");
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("save 0 null defineuserobject"),
        Err(PsError::MemoryExceeded),
        "L3"
    );
    let (count, _, len) = state_after(&mut it);
    assert_eq!((count, len), (3, 0), "L3: nothing installed");

    // L4: a growth builds its final capacity at once. The old array is
    // kept, so a copy-then-resize (which doubles the capacity) would hold
    // twice what the new array needs.
    let mut it = Interp::new();
    it.run_str("userdict /UserObjects 65534 array put /old userdict /UserObjects get def")
        .expect("an array");
    it.set_limits(Limits::none().with_max_memory(65536 * slot * 3 / 2))
        .expect("the allocator is installed");
    it.run_str("65535 null defineuserobject")
        .expect("the new array is one allocation of 65536 slots");
    let (_, _, len) = state_after(&mut it);
    assert_eq!(len, 65536);
}
